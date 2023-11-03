set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

build-win:
    . .\pre-build-win.ps1
    cargo build --release
install-win:
    . .\pre-build-win.ps1
    cargo build --release
    Copy-Item -Path {{ justfile_directory() }}\target\release\smrec.exe -Destination {{ env_var_or_default("USERPROFILE", "") }}\.cargo\bin\

prepare-linux:
    ./pre-build-linux.sh