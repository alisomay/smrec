param (
    [switch]$ci
)

Write-Output "============================================================================="
Write-Output "Pre build script for cpal asio feature."
Write-Output "Make sure that you have sourced this script instead of directly executing it or if in CI environment, pass the --ci flag."
Write-Output "============================================================================="

function Write-Env {
    <#
    .SYNOPSIS
    Sets an environment variable either in the current process or in the GITHUB_ENV file.
    
    .DESCRIPTION
    This function sets an environment variable. If the --ci switch is specified when 
    running the script, it writes the environment variable to the GITHUB_ENV file 
    so it is available to subsequent steps in a GitHub Actions workflow. Otherwise, 
    it sets the environment variable in the current process.
    
    .PARAMETER name
    The name of the environment variable to set.
    
    .PARAMETER value
    The value to set the environment variable to.
    
    .EXAMPLE
    Write-Env "MY_VARIABLE" "Some Value"
    
    This example sets the MY_VARIABLE environment variable to "Some Value" in the current process.
    
    .EXAMPLE
    .\pre-build-win.ps1 --ci
    Write-Env "MY_VARIABLE" "Some Value"
    
    This example, when run within the script with the --ci switch, writes the MY_VARIABLE 
    environment variable to the GITHUB_ENV file with a value of "Some Value".
    #>
    param (
        [string]$name,
        [string]$value
    )
    if ($ci) {
        Write-Output "$name=$value" | Out-File -FilePath $env:GITHUB_ENV -Append
    }
    else {
        Write-Output "Setting $name=$value"
        [System.Environment]::SetEnvironmentVariable($name, $value, "Process")
    }
}

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
            "*ARM64*" { "arm64" }
            "*ARM*" { "arm" }
            default { "amd64" }
        }
    }
    else { 
        "x86" 
    }

    Write-Output "Architecture detected as $arch."


    # Define search paths based on architecture
    # Will be overridden if CI flag is set
    $paths = @('C:\Program Files (x86)\Microsoft Visual Studio\', 'C:\Program Files\Microsoft Visual Studio\')

    # Find Visual Studio version number (only when CI flag is set)
    # TODO: This can be more robust and improve.. but later..
    if ($ci) {
        Write-Output "Searching for Visual Studio version number..."
        $vsVersion = Get-ChildItem 'C:\Program Files (x86)\Microsoft Visual Studio\' -Directory |
        Where-Object { $_.Name -match '\d{4}' } |
        Select-Object -ExpandProperty Name -First 1

        if (-not $vsVersion) {
            $vsVersion = Get-ChildItem 'C:\Program Files\Microsoft Visual Studio\' -Directory |
            Where-Object { $_.Name -match '\d{4}' } |
            Select-Object -ExpandProperty Name -First 1
        }
        
        if ($vsVersion) {
            Write-Output "Visual Studio version $vsVersion detected."
            $paths = 
            @(
                "C:\Program Files (x86)\Microsoft Visual Studio\$vsVersion\Community\VC\Auxiliary\Build\", 
                "C:\Program Files\Microsoft Visual Studio\$vsVersion\Community\VC\Auxiliary\Build\",
                "C:\Program Files (x86)\Microsoft Visual Studio\$vsVersion\Professional\VC\Auxiliary\Build\", 
                "C:\Program Files\Microsoft Visual Studio\$vsVersion\Professional\VC\Auxiliary\Build\",
                "C:\Program Files (x86)\Microsoft Visual Studio\$vsVersion\Enterprise\VC\Auxiliary\Build\", 
                "C:\Program Files\Microsoft Visual Studio\$vsVersion\Enterprise\VC\Auxiliary\Build\"
            )
          
        }
        else {
            Write-Output "Visual Studio version not detected. Proceeding with original search paths."
            $paths = @('C:\Program Files (x86)\Microsoft Visual Studio\', 'C:\Program Files\Microsoft Visual Studio\')
        }
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
                    Write-Env $varName $varValue
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
    }
    else {
        Write-Output "ASIO SDK already exists. Skipping download."
    }

    # Set the CPAL_ASIO_DIR environment variable
    Write-Output "Setting CPAL_ASIO_DIR environment variable..."
    Write-Env "CPAL_ASIO_DIR" $asio_dir

    # Check if LIBCLANG_PATH is set
    if (-not $env:LIBCLANG_PATH) {
        Write-Error "Error: LIBCLANG_PATH is not set!"
        Write-Output "Please ensure LLVM is installed and set the LIBCLANG_PATH environment variable."
        exit 1
    }
    else {
        Write-Output "LIBCLANG_PATH is set to $env:LIBCLANG_PATH."
    }

    # Run the vcvars function
    Invoke-VcVars

    Write-Output "Environment is ready for build."
}
else {
    Write-Output "This setup script is intended for Windows only."
}
