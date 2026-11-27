#!/bin/bash
set -e

while (($# > 0)); do
  case $1 in
  --test) # install test dependencies
    TEST=1
    shift
    ;;
  esac
done

OS=$(uname -s)
ARCH=$(uname -m)

install_python_provider() {
  CC=cc python3 -m pip -q install --user --upgrade --break-system-packages \
    --require-hashes -r .github/ci/provider/requirements.txt
}

install_node_provider() {
  local node_dir="${NPM_CONFIG_PREFIX:-$PWD/.deps/node}/lib"
  mkdir -p "$node_dir"
  cp .github/ci/provider/package.json .github/ci/provider/package-lock.json "$node_dir/"
  npm ci --prefix "$node_dir" --no-audit --no-fund
  if [[ -n ${GITHUB_PATH:-} ]]; then
    echo "$node_dir/node_modules/.bin" >> "$GITHUB_PATH"
  fi
}

if [[ $OS == Linux ]]; then
  sudo apt-get update
  sudo apt-get install -y build-essential curl gettext

  if [[ -n $TEST ]]; then
    sudo apt-get install -y locales-all carton attr libattr1-dev fish gdb inotify-tools xdg-utils

    # Use default CC to avoid compilation problems when installing Python modules
    install_python_provider

    # Skip installing npm on aarch64 as it tends to cause intermittent segmentation faults.
    # See https://github.com/neovim/neovim/issues/32339.
    if [[ $ARCH != aarch64 ]]; then
      install_node_provider
    fi
  fi
elif [[ $OS == Darwin ]]; then
  if [[ -n $TEST ]]; then
    brew untap aws/tap
    brew install carton fish fswatch

    install_node_provider

    # Use default CC to avoid compilation problems when installing Python modules
    install_python_provider
  fi
fi
