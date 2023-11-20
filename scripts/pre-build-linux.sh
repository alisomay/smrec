#!/bin/bash

# Determine the package manager
if command -v apt-get >/dev/null 2>&1; then
    PACKAGE_MANAGER="apt-get"
elif command -v dnf >/dev/null 2>&1; then
    PACKAGE_MANAGER="dnf"
elif command -v yum >/dev/null 2>&1; then
    PACKAGE_MANAGER="yum"
elif command -v zypper >/dev/null 2>&1; then
    PACKAGE_MANAGER="zypper"
elif command -v pacman >/dev/null 2>&1; then
    PACKAGE_MANAGER="pacman"
else
    echo "Unsupported package manager. Exiting."
    exit 1
fi

# Define the package names
if [[ "$PACKAGE_MANAGER" == "apt-get" ]]; then
    ALSA_PACKAGE="libasound2-dev"
    JACK_PACKAGE="libjack-jackd2-dev"
elif [[ "$PACKAGE_MANAGER" == "dnf" || "$PACKAGE_MANAGER" == "yum" ]]; then
    ALSA_PACKAGE="alsa-lib-devel"
    JACK_PACKAGE="jack-audio-connection-kit-devel"
elif [[ "$PACKAGE_MANAGER" == "zypper" ]]; then
    ALSA_PACKAGE="alsa-devel"
    JACK_PACKAGE="jack-devel"
elif [[ "$PACKAGE_MANAGER" == "pacman" ]]; then
    ALSA_PACKAGE="alsa-lib"
    JACK_PACKAGE="jack"
fi

# Update package lists for the latest version of the repository
if [[ "$PACKAGE_MANAGER" == "pacman" ]]; then
    echo "Updating package databases..."
    sudo pacman -Syq --noconfirm || {
        echo "Updating package databases failed. Exiting."
        exit 1
    }
else
    echo "Updating package lists..."
    sudo $PACKAGE_MANAGER update -yq || {
        echo "Updating package lists failed. Exiting."
        exit 1
    }
fi

# Install necessary packages for ALSA
echo "Installing ALSA development files..."
sudo $PACKAGE_MANAGER install -yq $ALSA_PACKAGE || {
    echo "Installing ALSA development files failed. Exiting."
    exit 1
}

# Install necessary packages for JACK
echo "Installing JACK development files..."
sudo $PACKAGE_MANAGER install -yq $JACK_PACKAGE || {
    echo "Installing JACK development files failed. Exiting."
    exit 1
}

echo "Build environment preparation complete."
