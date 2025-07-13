# srt-igt-splits

## Dependencies

- For rusty-tesseract: install tesseract and have it on PATH
- For tesseract:
    - install vcpkg (set VCPKG_ROOT and(/or?) have it on PATH)
    - ./vcpkg install leptonica:x64-windows-static-md tesseract:x64-windows-static-md
    - winget install LLVM
    - it seems the "tessdata" from the regular tesseract installation is still required, and TESSDATA_PREFIX needs to be set accordingly