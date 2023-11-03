ARG CROSS_BASE_IMAGE
FROM $CROSS_BASE_IMAGE

# Redefine ARG after FROM
ARG CROSS_DEB_ARCH
ARG DEBIAN_FRONTEND=noninteractive

# Update and upgrade the base system
RUN apt-get update -y \
    && apt-get upgrade -y \
    && apt-get dist-upgrade -y

# Install aptitude for more robust dependency resolution
RUN apt-get install -y aptitude

# Run the command to get the current version of libc6 from focal-updates/main, store the result in a variable,
# and then write the export statement with that version to the envfile.
RUN export HOST_LIBC_VERSION=$(apt-cache madison libc6 | grep 'focal-updates/main' | awk '{print $3;}' | head -1) && \
    echo "export HOST_LIBC_VERSION=$HOST_LIBC_VERSION" > /envfile

# Add sources for the specified architecture
RUN if echo "$CROSS_DEB_ARCH" | grep -q "arm"; then \
    echo "deb [arch=$CROSS_DEB_ARCH] http://ports.ubuntu.com/ubuntu-ports/ focal main restricted universe multiverse" > /etc/apt/sources.list && \
    echo "deb [arch=$CROSS_DEB_ARCH] http://ports.ubuntu.com/ubuntu-ports/ focal-updates main restricted universe multiverse" >> /etc/apt/sources.list && \
    echo "deb [arch=$CROSS_DEB_ARCH] http://ports.ubuntu.com/ubuntu-ports/ focal-backports main restricted universe multiverse" >> /etc/apt/sources.list && \
    echo "deb [arch=$CROSS_DEB_ARCH] http://ports.ubuntu.com/ubuntu-ports/ focal-security main restricted universe multiverse" >> /etc/apt/sources.list; \
    else \
    echo "deb [arch=$CROSS_DEB_ARCH] http://archive.ubuntu.com/ubuntu/ focal main restricted universe multiverse" > /etc/apt/sources.list && \
    echo "deb [arch=$CROSS_DEB_ARCH] http://archive.ubuntu.com/ubuntu/ focal-updates main restricted universe multiverse" >> /etc/apt/sources.list && \
    echo "deb [arch=$CROSS_DEB_ARCH] http://archive.ubuntu.com/ubuntu/ focal-backports main restricted universe multiverse" >> /etc/apt/sources.list && \
    echo "deb [arch=$CROSS_DEB_ARCH] http://security.ubuntu.com/ubuntu/ focal-security main restricted universe multiverse" >> /etc/apt/sources.list; \
    fi

RUN dpkg --add-architecture ${CROSS_DEB_ARCH} && apt-get update -y

RUN cat /envfile;

# # Update and install dependencies using bash shell for better subshell handling
# RUN apt-get update -y && \
#     apt-get install -y && \
#     . /envfile; libc6:${CROSS_DEB_ARCH}=$HOST_LIBC_VERSION 

# Install build dependencies
RUN aptitude install -y libasound2-dev:${CROSS_DEB_ARCH} libjack-jackd2-dev:${CROSS_DEB_ARCH}
