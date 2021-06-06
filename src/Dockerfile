FROM archlinux

# Add special configuration for bash.
ADD https://raw.githubusercontent.com/airvzxf/archLinux-installer-and-setup/master/src/laptop_MSI_GT73EVR_7R_Titan_Pro/04-setup/setup-resources/.bashrc /root

# Add better mirror server
RUN echo "Server = https://arch.mirror.constant.com/\$repo/os/\$arch" > /etc/pacman.d/mirrorlist
RUN echo "Server = https://america.mirror.pkgbuild.com/\$repo/os/\$arch" >> /etc/pacman.d/mirrorlist

# Up date the system
RUN pacman -Syyu --noconfirm

# Install packages
RUN pacman -S --noconfirm \
        screenfetch \
        gcc \
        make \
        cmake \
        pkgconf \
        clang \
        cppcheck \
        bluez-libs

# Add new user with the same ID's as the host
ARG USER_ID
ARG GROUP_ID
RUN echo "USER_ID:  ${USER_ID}"
RUN echo "GROUP_ID: ${GROUP_ID}"
RUN groupadd \
    --non-unique \
    --force \
    --gid ${GROUP_ID} \
    boseconnect
RUN useradd \
    --no-log-init \
    --no-create-home \
    --non-unique \
    --uid ${USER_ID} \
    --gid ${GROUP_ID} \
    --groups root \
    boseconnect
