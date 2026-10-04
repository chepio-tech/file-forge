; Keep the company signature opposite the native page title, without replacing Tauri's installer template.
!define MUI_HEADERIMAGE_RIGHT
; Preserve artwork proportions when Windows scales the native controls for the user's DPI/font settings.
!define MUI_HEADERIMAGE_BITMAP_STRETCH "AspectFitHeight"
!define MUI_HEADERIMAGE_UNBITMAP_STRETCH "AspectFitHeight"
!define MUI_WELCOMEFINISHPAGE_BITMAP_STRETCH "AspectFitHeight"

; MUI's aspect-fit helper resizes the STATIC control, leaving Windows to stretch the source bitmap.
; At fractional DPI this aliases the wordmark. Keep MUI's sizing/alignment, then give the control a
; prefiltered 24-bit bitmap at its exact physical size. MUI still owns page cleanup and the stock template.
!macroundef MUI_LOADANDASPECTSTRETCHIMAGETOCONTROLHEIGHT
!macro MUI_LOADANDASPECTSTRETCHIMAGETOCONTROLHEIGHT CONTROL PATH ALIGN HANDLE
  !insertmacro MUI_INTERNAL_LOADANDSIZEIMAGE MUI_INTERNAL_LOADANDASPECTSTRETCHIMAGETOCONTROLHEIGHT "${CONTROL}" "${PATH}" "${ALIGN}" Stack
  Push "${CONTROL}"
  ${CallArtificialFunction} FileForgeSmoothBitmap
  !if "${HANDLE}" == "Leak"
    !insertmacro _LOGICLIB_TEMP
    Pop $_LOGICLIB_TEMP
  !else if "${HANDLE}" != "Stack"
    Pop ${HANDLE}
  !endif
!macroend

!macro FileForgeSmoothBitmap
  System::Store "S"
  Pop $0 ; Control HWND
  Pop $1 ; Original bitmap; return it unchanged on failure.
  ${If} $1 P<> 0
    System::Alloc 32 ; BITMAP fits both the 32-bit and 64-bit layouts.
    Pop $R2
    ${If} $R2 P<> 0
      System::Call 'GDI32::GetObjectW(pr1,i32,pR2)i.R3'
      System::Call '*$R2(i,i.r2,i.r3)'
      System::Call 'USER32::GetClientRect(pr0,pR2)i.R4'
      System::Call '*$R2(i,i,i.r4,i.r5)'
      System::Free $R2
      ${If} $R3 > 0
      ${AndIf} $R4 <> 0
      ${AndIf} $4 > 0
      ${AndIf} $5 > 0
        ${If} $2 <> $4
        ${OrIf} $3 <> $5
          System::Call 'GDI32::CreateCompatibleDC(p0)p.r6'
          System::Call 'GDI32::CreateCompatibleDC(p0)p.r7'
          ; BITMAPINFOHEADER: uncompressed RGB, no alpha that STM_SETIMAGE could copy implicitly.
          System::Call '*(i40,ir4,ir5,&i2 1,&i2 24,i0,i0,i0,i0,i0,i0)p.r9'
          System::Call 'GDI32::CreateDIBSection(p0,pr9,i0,*p.R2,p0,i0)p.r8'
          System::Free $9
          StrCpy $R3 0
          ${If} $6 P<> 0
          ${AndIf} $7 P<> 0
          ${AndIf} $8 P<> 0
            System::Call 'GDI32::SelectObject(pr6,pr1)p.R0'
            System::Call 'GDI32::SelectObject(pr7,pr8)p.R1'
            System::Call 'GDI32::SetStretchBltMode(pr7,i4)i.R3' ; HALFTONE
            ${If} $R3 <> 0
              System::Call 'GDI32::SetBrushOrgEx(pr7,i0,i0,p0)i.R3'
              ${If} $R3 <> 0
                System::Call 'GDI32::StretchBlt(pr7,i0,i0,ir4,ir5,pr6,i0,i0,ir2,ir3,i0x00CC0020)i.R3'
              ${EndIf}
            ${EndIf}
            System::Call 'GDI32::SelectObject(pr6,pR0)'
            System::Call 'GDI32::SelectObject(pr7,pR1)'
          ${EndIf}
          ${If} $6 P<> 0
            System::Call 'GDI32::DeleteDC(pr6)'
          ${EndIf}
          ${If} $7 P<> 0
            System::Call 'GDI32::DeleteDC(pr7)'
          ${EndIf}
          ${If} $R3 <> 0
            SendMessage $0 ${STM_SETIMAGE} ${IMAGE_BITMAP} $8 $R2
            ${If} $R2 P<> 0
            ${AndIf} $R2 P<> $1
              System::Call 'GDI32::DeleteObject(pR2)'
            ${EndIf}
            System::Call 'GDI32::DeleteObject(pr1)'
            StrCpy $1 $8
          ${ElseIf} $8 P<> 0
            System::Call 'GDI32::DeleteObject(pr8)'
          ${EndIf}
        ${EndIf}
      ${EndIf}
    ${EndIf}
  ${EndIf}
  Push $1
  System::Store "L"
!macroend
