.PHONY: podman clean all

all: podman

podman:
	podman-compose -f deploy/podman/podman-compose.yaml up -d

clean:
	podman-compose -f deploy/podman/podman-compose.yaml down -v
