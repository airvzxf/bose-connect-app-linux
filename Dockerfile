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
        make \
        cmake \
        pkgconf \
        clang \
        cppcheck \
        bluez-libs

# Copy application
COPY ./src /root/bose-connect-app-linux

# Set up the app
WORKDIR /root/bose-connect-app-linux
RUN cmake \
        -S . \
        -B ./build \
        -DCMAKE_BUILD_TYPE=Release \
        -DVALIDATE_QA=True # Optional: It could remove.
RUN cmake \
      --build ./build \
      --parallel $(nproc)
RUN cd build && \
    ctest -C Release && \
    cd ../
