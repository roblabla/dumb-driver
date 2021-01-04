#![no_std]

use core::panic::PanicInfo;
use core::ptr::null_mut;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;
use core::mem::{MaybeUninit, size_of};
use winapi::km::fltkernel::*;
use winapi::km::wdm::*;
use winapi::shared::ntdef::*;
use winapi::shared::ntstatus::*;
use utf16_lit::utf16;

// This is fine because we don't actually have any floating point instruction in
// our binary, thanks to our target defining soft-floats. fltused symbol is
// necessary due to LLVM being too eager to set it: it checks the LLVM IR for
// floating point instructions - even if soft-float is enabled!
#[no_mangle]
static _fltused: () = ();

struct FltRegistration(FLT_REGISTRATION);

unsafe impl Send for FltRegistration {}
unsafe impl Sync for FltRegistration {}

static FILTER_REGISTRATION: FltRegistration = FltRegistration(FLT_REGISTRATION {
    Size: size_of::<FLT_REGISTRATION>() as u16,
    Version: FLT_REGISTRATION_VERSION,
    Flags: 0,
    ContextRegistration: null_mut(),
    OperationRegistration: &FLT_OPERATION_REGISTRATION {
        MajorFunction: IRP_MJ_OPERATION_END,
        Flags: 0,
        PreOperation: None,
        PostOperation: None,
        Reserved1: null_mut(),
    },
    FilterUnloadCallback: Some(filter_unload_callback),
    InstanceSetupCallback: Some(instance_setup_callback),
    InstanceQueryTeardownCallback: None,
    InstanceTeardownStartCallback: None,
    InstanceTeardownCompleteCallback: None,
    GenerateFileNameCallback: null_mut(),
    NormalizeNameComponentCallback: null_mut(),
    NormalizeContextCleanupCallback: None,
    TransactionNotificationCallback: null_mut(),
    NormalizeNameComponentExCallback: null_mut(),
    SectionNotificationCallback: null_mut(),
});

unsafe extern "system" fn filter_unload_callback(_flags: FLT_FILTER_UNLOAD_FLAGS) -> NTSTATUS {
    FltCloseCommunicationPort(FILTER_SERVER_PORT_HANDLE);
    FltUnregisterFilter(FILTER_HANDLE);
    STATUS_SUCCESS
}

unsafe extern "system" fn instance_setup_callback(
    _flt_objects: PCFLT_RELATED_OBJECTS,
    _flags: FLT_INSTANCE_SETUP_FLAGS,
    _volume_device_type: ULONG,
    _volume_filesystem_fype: FLT_FILESYSTEM_TYPE,
) -> NTSTATUS
{
    STATUS_SUCCESS
}

macro_rules! make_unicode {
    ($s:expr) => {{
        const U16_S: &[u16] = $s;
        const S: UNICODE_STRING = UNICODE_STRING {
            Length: U16_S.len() as u16,
            MaximumLength: U16_S.len() as u16,
            Buffer: U16_S.as_ptr() as _,
        };
        S
    }}
}

static mut FILTER_HANDLE: PFLT_FILTER = null_mut();
static mut FILTER_SERVER_PORT_HANDLE: PFLT_PORT = null_mut();
static mut FILTER_CLIENT_PORT_HANDLE: PFLT_PORT = null_mut();
static mut DATA: [u8; 512] = [0; 512];

#[no_mangle]
pub unsafe extern "system" fn driver_entry(
    driver_object: PDRIVER_OBJECT,
    _registry_path: PUNICODE_STRING
) -> NTSTATUS
{
    // TODO: ExInitializeDriverRuntime(DrvRtPoolNxOptIn)
    (*driver_object).DriverUnload = Some(driver_unload);

    /*let ret = IoCreateDevice(
        driver_object,
        0,
        NULL,
        FILE_DEVICE_TRANSPORT,
        FILE_DEVICE_SECURE_OPEN,
        FALSE,
        &device,
    );

    if !NT_SUCCESS(ret) {
        // unload
        return ret;
    }*/

    // Create a minifilter
    let ret = FltRegisterFilter(
        driver_object,
        &FILTER_REGISTRATION.0,
        &mut FILTER_HANDLE,
    );
    if !NT_SUCCESS(ret) {
        // unload
        return ret;
    }


    // Create the security descriptor to pass to FltCreateCommunicationPort.
    let mut sd = null_mut();
    let ret = FltBuildDefaultSecurityDescriptor(
        &mut sd,
        FLT_PORT_ALL_ACCESS);
    if !NT_SUCCESS(ret) {
        // unload
        return ret;
    }
    let mut oa = MaybeUninit::<OBJECT_ATTRIBUTES>::zeroed().assume_init();
    InitializeObjectAttributes(
        &mut oa,
        &mut make_unicode!(&utf16!(r"\TestFilterPort")),
        OBJ_KERNEL_HANDLE | OBJ_CASE_INSENSITIVE,
        null_mut(),
        sd);

    // Create a communication port.
    let ret = FltCreateCommunicationPort(
        FILTER_HANDLE,
        &mut FILTER_SERVER_PORT_HANDLE,
        &mut oa,
        null_mut(),
        Some(connect_callback),
        Some(disconnect_callback),
        Some(message_callback),
        1,
    );

    FltFreeSecurityDescriptor(sd);

    if !NT_SUCCESS(ret) {
        // unload
        return ret;
    }

    // Create a thread
    let mut oa = MaybeUninit::<OBJECT_ATTRIBUTES>::zeroed().assume_init();
    // now create thread that will consume entries in linked list
    InitializeObjectAttributes(
        &mut oa,
        null_mut(),
        OBJ_KERNEL_HANDLE,
        null_mut(),
        null_mut());

    let mut thread = null_mut();
    let status = PsCreateSystemThread(
        &mut thread,
        THREAD_ALL_ACCESS,
        &mut oa,
        null_mut(),
        null_mut(),
        Some(thread_fn),
        null_mut(),
    );

    if !NT_SUCCESS(status) {
        return status
    }

    STATUS_SUCCESS
}

static SHOULD_STOP: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn thread_fn(_ctx: PVOID) {
    let mut ret = 0;
    while !SHOULD_STOP.load(Ordering::SeqCst) {
        let mut should_send_without_reply = 0u8;
        let mut reply_len = size_of::<u8>() as u32;
        ret = FltSendMessage(
            FILTER_HANDLE,
            &mut FILTER_CLIENT_PORT_HANDLE,
            DATA.as_mut_ptr() as _,
            DATA.len() as _,
            &mut should_send_without_reply as *mut u8 as _,
            &mut reply_len,
            null_mut(),
        );

        if ret != STATUS_SUCCESS {
            break;
        }
    }
    PsTerminateSystemThread(ret);
}

unsafe extern "system" fn driver_unload(_driver_object: PDRIVER_OBJECT) {
}

unsafe extern "system" fn connect_callback(
    client_port: PFLT_PORT,
    _server_port_cookie: PVOID,
    connection_context: PVOID,
    size_of_context: ULONG,
    _connection_port_cookie: *mut PVOID,
) -> NTSTATUS
{
    FILTER_CLIENT_PORT_HANDLE = client_port;
    let initial_data = connection_context as *mut u8;
    let initial_size = size_of_context as usize;
    let initial_data = core::slice::from_raw_parts_mut(initial_data, initial_size);

    if initial_data.len() >= 512 {
        return STATUS_BUFFER_OVERFLOW;
    }

    /*let ret = FltSendMessage(
        FILTER_HANDLE,
        client_port,
        SenderBuffer,
        SenderBufferLength,
        ReplyBuffer,
        ReplyLength,
        null_mut(),
    );*/

    return STATUS_SUCCESS;
}

unsafe extern "system" fn disconnect_callback(
    _connection_cookie: PVOID,
) -> ()
{
    FltCloseClientPort(FILTER_HANDLE, &mut FILTER_CLIENT_PORT_HANDLE);
    FILTER_CLIENT_PORT_HANDLE = null_mut();
}

unsafe extern "system" fn message_callback(
    _port_cookie: PVOID,
    _input_buffer: PVOID,
    _input_buffer_length: ULONG,
    _output_buffer_: PVOID,
    _output_buffer_length: ULONG,
    _return_output_buffer_length: PULONG,
) -> NTSTATUS
{
    STATUS_SUCCESS
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
    // TODO: KeBugCheck();
}