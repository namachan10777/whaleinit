
runtime := "docker"
version := "0.0.1"

default: test

build-test-image:
    {{runtime}} build -t ghcr.io/namachan10777/whaleinit-test:latest -f test/Dockerfile .

test: build-test-image
    {{runtime}} run -e TEST_SERVICE_NAME="test service" -e TEST_MSG="This is test" -it ghcr.io/namachan10777/whaleinit-test:latest
