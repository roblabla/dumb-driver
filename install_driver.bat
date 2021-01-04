certutil -delstore PrivateCertStore Roblabla(Test)
certutil -delstore Root Roblabla(Test)
certutil -delstore TrustedPublisher Roblabla(Test)
"C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\makecert" -r -pe -ss PrivateCertStore -n CN=Roblabla(Test) -eku 1.3.6.1.5.5.7.3.3 cert.cer
certutil -addstore Root cert.cer
certutil -addstore TrustedPublisher cert.cer
"C:\Program Files (x86)\Windows Kits\10\bin\x86\Inf2Cat" /driver:. /os:10_19H1_X64
"C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\Signtool" sign /v /fd sha256 /s PrivateCertStore /n Roblabla(Test) /t http://timestamp.digicert.com windows_driver_test.cat
"C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\Signtool" verify /pa /v /c windows_driver_test.cat windows_driver_test.inf
rundll32 setupapi.dll,InstallHinfSection "DefaultInstall 132 windows_driver_test.inf"