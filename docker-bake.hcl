variable "VERSION" {
  default = "0.0.1"
}

target "default" {
  dockerfile = "images/ubuntu-noble/Dockerfile"
  tags = [
    "ghcr.io/namachan10777/whaleinit:${VERSION}",
    "ghcr.io/namachan10777/whaleinit:latest",
  ]
  platforms = ["linux/amd64", "linux/arm64"]
}

target "ubuntu-noble" {
  dockerfile = "images/ubuntu-noble/Dockerfile"
  tags = [
    "ghcr.io/namachan10777/whaleinit:ubuntu-noble",
    "ghcr.io/namachan10777/whaleinit:ubuntu-noble-${VERSION}"
  ]
  platforms = ["linux/amd64", "linux/arm64"]
}
