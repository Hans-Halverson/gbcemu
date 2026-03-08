#!/bin/bash

set -e

sudo apt-get update
sudo apt-get install -y \
  libasound2-dev \
  libgdk-pixbuf2.0-dev \
  libglib2.0-dev \
  libgtk-3-dev \
  libpango1.0-dev \
  libxdo-dev