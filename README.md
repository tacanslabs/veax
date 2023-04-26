# VEAX smart contract repository

## Development environment

To prepare the Docker environment please use official documentation:

[https://docs.docker.com/engine/install/](https://docs.docker.com/engine/install/)

To build debug artifacts locally please follow the instructions [here](infra/os_setup/README.md).

### How to build release artifact using Docker

```sh
cd veax/dex
./build-wasm-release.sh
```

### How to build debug artifact locally

```sh
cd veax/dex
./build-wasm-debug.sh
```
