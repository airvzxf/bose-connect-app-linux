FROM archlinux:latest

RUN pacman -Sy
#RUN pacman -Sy && \
#    pacman --noconfirm -S \
#        cmake \
#        clang \
#        cppcheck \
#        bluez-libs

COPY ./src /root/bose-connect-app-linux

WORKDIR /root/bose-connect-app-linux

RUN cmake \
        -S . \
        -B ./build \
        -DCMAKE_BUILD_TYPE=Release

RUN cmake \
        --build ./build
