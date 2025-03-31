variable "VERSION" {
  default = "0.0.1"
}

target "default" {
  dockerfile = "images/ubuntu-jammy/Dockerfile"
  tags = [
    "ghcr.io/namachan10777/whaleinit:${VERSION}",
    "ghcr.io/namachan10777/whaleinit:latest",
  ]
  platforms = ["linux/amd64", "linux/arm64"]
}

target "ubuntu-jammy" {
  dockerfile = "images/ubuntu-jammy/Dockerfile"
  tags = [
    "ghcr.io/namachan10777/whaleinit:ubuntu-jammy",
    "ghcr.io/namachan10777/whaleinit:ubuntu-jammy-${VERSION}"
  ]
  platforms = ["linux/amd64", "linux/arm64"]
}
