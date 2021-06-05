FROM airvzxf/bose-connect-app-linux-os

# Copy application
COPY ./src /root/bose-connect-app-linux

# Set up the app
WORKDIR /root/bose-connect-app-linux
RUN cmake \
        -S . \
        -B ./build \
        -DCMAKE_BUILD_TYPE=Release
RUN cmake \
      --build ./build \
      --parallel $(nproc)
RUN cd build && \
    ctest -C Release && \
    cd ../
