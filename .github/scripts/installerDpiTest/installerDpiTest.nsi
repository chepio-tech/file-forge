Unicode true
ManifestDPIAware true
ManifestDPIAwareness PerMonitorV2
RequestExecutionLevel user
Name "FileForge installer DPI test"
OutFile "${TEST_DIRECTORY}\${TEST_MODE}.exe"

; Core
!include MUI2.nsh
; Components
!ifdef SMOOTH
  !include "${REPOSITORY_ROOT}\src-tauri\branding\installer-branding\installerBranding.nsh"
!endif

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var Report
Var TestHeight
Var GrayPixels
Var BeforeGdi
Var Round

Function .onInit
  InitPluginsDir
  File /oname=$PLUGINSDIR\checker.bmp "${TEST_DIRECTORY}\checker.bmp"
  FileOpen $Report "${TEST_DIRECTORY}\${TEST_MODE}.txt" w
  StrCpy $Round 0
  System::Call 'KERNEL32::GetCurrentProcess()p.r0'
  System::Call 'USER32::GetGuiResources(pr0,i0)i.r1'
  StrCpy $BeforeGdi $1
  repeat:
    StrCpy $TestHeight 4
    size:
      ; Exact physical control sizes for a 2×, 8-pixel source at 100%, 150% and 200%.
      System::Call 'USER32::CreateWindowExW(i0,t"STATIC",t"",i0x8000000E,i0,i0,i$TestHeight,i$TestHeight,p0,p0,p0,p0)p.r0'
      !insertmacro MUI_LOADANDASPECTSTRETCHIMAGETOCONTROLHEIGHT $0 "$PLUGINSDIR\checker.bmp" Left $1
      System::Alloc 32
      Pop $2
      System::Call 'GDI32::GetObjectW(pr1,i32,pr2)i.r9'
      System::Call '*$2(i,i.r3,i.r4)'
      System::Free $2
      System::Call 'GDI32::CreateCompatibleDC(p0)p.r2'
      System::Call 'GDI32::SelectObject(pr2,pr1)p.r8'
      StrCpy $GrayPixels 0
      StrCpy $5 0
      row:
        StrCpy $6 0
        column:
          System::Call 'GDI32::GetPixel(pr2,ir6,ir5)i.r7'
          ; Integer comparisons: LogicLib's != compares strings, and GetPixel returns decimal (white = 16777215).
          ${If} $7 <> 0
          ${AndIf} $7 <> 0xFFFFFF
          ${AndIf} $7 <> -1
            IntOp $GrayPixels $GrayPixels + 1
          ${EndIf}
          IntOp $6 $6 + 1
          IntCmp $6 $3 0 column
        IntOp $5 $5 + 1
        IntCmp $5 $4 0 row
      System::Call 'GDI32::SelectObject(pr2,pr8)'
      System::Call 'GDI32::DeleteDC(pr2)'
      System::Call 'USER32::DestroyWindow(pr0)'
      System::Call 'GDI32::DeleteObject(pr1)'
      ${If} $Round == 0
        FileWrite $Report "$TestHeight,$3,$4,$GrayPixels$\r$\n"
      ${EndIf}
      IntOp $TestHeight $TestHeight + 2
      IntCmp $TestHeight 8 size size
    IntOp $Round $Round + 1
    IntCmp $Round 40 0 repeat
  System::Call 'KERNEL32::GetCurrentProcess()p.r0'
  System::Call 'USER32::GetGuiResources(pr0,i0)i.r1'
  IntOp $1 $1 - $BeforeGdi
  FileWrite $Report "gdi,$1$\r$\n"
  FileClose $Report
  SetErrorLevel 0
  Quit
FunctionEnd

Section
  WriteUninstaller "$PLUGINSDIR\uninstall.exe"
SectionEnd
Section "Uninstall"
SectionEnd
