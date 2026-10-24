# Clean Ubuntu 24.04 machines for e2e/turnkey/run.sh. Build for linux/amd64:
# the pinned llama-server release has x86_64 Linux binaries only.

# A new install before anything is set up: the default `ubuntu` user with sudo,
# and nothing else. No compiler, git, curl, Node.js or Rust.
FROM ubuntu:24.04 AS fresh
RUN apt-get update \
 && apt-get install -y --no-install-recommends sudo \
 && rm -rf /var/lib/apt/lists/* \
 && echo 'ubuntu ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/ubuntu \
 && chmod 0440 /etc/sudoers.d/ubuntu
ENV LANG=C.UTF-8
USER ubuntu
WORKDIR /home/ubuntu

# The same machine with a display, for the first launch. Xvfb stands in for
# the desktop session and fonts-dejavu for the fonts every desktop ships. The
# rest is the test's own tooling: xdotool clicks and types, ImageMagick takes
# screenshots, Tesseract finds text on them, sqlite3 and python3 read the
# app's database. None of it is a dependency of the app; apt has to pull
# those from the .deb's own Depends.
FROM fresh AS desktop
USER root
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      xvfb xdotool imagemagick tesseract-ocr tesseract-ocr-eng \
      sqlite3 python3 dbus fonts-dejavu-core ca-certificates procps \
 && rm -rf /var/lib/apt/lists/*
USER ubuntu
