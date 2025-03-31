
default: test

build-test-image:
    podman build -t ghcr.io/namachan10777/whaleinit-test:latest -f test/Dockerfile .

test: build-test-image
    podman run -it ghcr.io/namachan10777/whaleinit-test:latest
