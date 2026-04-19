# Docker

`docker-compose.yml` at the repository root uses **official language images** and **bind-mounts** each service submodule into the container. There are no bespoke Dockerfiles here on purpose: the source of truth stays inside each service repository.

First-time `make dev` compiles Rust and installs Ruby gems inside the containers (network required). After submodules are warm, rebuilds are faster.
