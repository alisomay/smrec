Write-Output "============================================================================="
Write-Output "Pre build script for cpal asio feature."
Write-Output "Make sure that you have sourced this script instead of directly executing it."
Write-Output "============================================================================="

function Invoke-VcVars {
    <#
    .SYNOPSIS
    This function sets the Visual Studio build environment for the current session.

    .DESCRIPTION
    The function first determines the system architecture. It then searches for the vcvarsall.bat file 
    specific to the detected architecture and executes it to set the Visual Studio build environment 
    for the current session.
    #>
    
    # Determine the system architecture
    Write-Output "Determining system architecture..."
    $arch = if ([Environment]::Is64BitOperatingSystem) {
        switch -Wildcard ((Get-CimInstance -ClassName Win32_Processor).Description) {
            "*ARM64*"{ "arm64" }
            "*ARM*"{ "arm" }
            default { "amd64" }
        }
    } else { 
        "x86" 
    }

    Write-Output "Architecture detected as $arch."

    # Define search paths based on architecture
    $paths = if ($arch -eq 'amd64') {
        @('C:\Program Files (x86)\Microsoft Visual Studio\', 'C:\Program Files\Microsoft Visual Studio\')
    } else {
        @('C:\Program Files\Microsoft Visual Studio\')
    }

    # Search for vcvarsall.bat and execute the first instance found with the appropriate architecture argument
    Write-Output "Searching for vcvarsall.bat..."
    foreach ($path in $paths) {
        $vcvarsPath = Get-ChildItem $path -Recurse -Filter vcvarsall.bat -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($vcvarsPath) {
            Write-Output "Found vcvarsall.bat at $($vcvarsPath.FullName). Initializing environment..."
            $cmdOutput = cmd /c """$($vcvarsPath.FullName)"" $arch && set"
            foreach ($line in $cmdOutput) {
                if ($line -match "^(.*?)=(.*)$") {
                    $varName = $matches[1]
                    $varValue = $matches[2]
                    [System.Environment]::SetEnvironmentVariable($varName, $varValue, "Process")
                }
            }
            return
        }
    }

    Write-Error "Error: Could not find vcvarsall.bat. Please install the latest version of Visual Studio."
    exit 1
}

# Main script begins here

# Ensure execution policy allows for script execution
if (-not (Get-ExecutionPolicy -Scope CurrentUser) -eq "Unrestricted") {
    Set-ExecutionPolicy -Scope CurrentUser Unrestricted
}

# Check if running on Windows
if ($env:OS -match "Windows") {
    Write-Output "Detected Windows OS."

    # Directory to store the ASIO SDK
    $out_dir = [System.IO.Path]::GetTempPath()
    $asio_dir = Join-Path $out_dir "asio_sdk"

    if (-not (Test-Path $asio_dir)) {
        Write-Output "ASIO SDK not found. Downloading..."
        
        # Download the ASIO SDK
        $asio_zip_path = Join-Path $out_dir "asio_sdk.zip"
        Invoke-WebRequest -Uri "https://www.steinberg.net/asiosdk" -OutFile $asio_zip_path

        # Unzip the ASIO SDK
        Write-Output "Unzipping ASIO SDK..."
        Expand-Archive -Path $asio_zip_path -DestinationPath $out_dir -Force

        # Move the contents of the inner directory (like asiosdk_2.3.3_2019-06-14) to $asio_dir
        $innerDir = Get-ChildItem -Path $out_dir -Directory | Where-Object { $_.Name -match 'asio.*' } | Select-Object -First 1
        Move-Item -Path "$($innerDir.FullName)\*" -Destination $asio_dir -Force
    } else {
        Write-Output "ASIO SDK already exists. Skipping download."
    }

    # Set the CPAL_ASIO_DIR environment variable
    Write-Output "Setting CPAL_ASIO_DIR environment variable..."
    $env:CPAL_ASIO_DIR = $asio_dir

    # Check if LIBCLANG_PATH is set
    if (-not $env:LIBCLANG_PATH) {
        Write-Error "Error: LIBCLANG_PATH is not set!"
        Write-Output "Please ensure LLVM is installed and set the LIBCLANG_PATH environment variable."
        exit 1
    } else {
        Write-Output "LIBCLANG_PATH is set to $env:LIBCLANG_PATH."
    }

    # Run the vcvars function
    Invoke-VcVars
} else {
    Write-Output "This setup script is intended for Windows only."
}
