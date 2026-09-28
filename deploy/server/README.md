# AIKS Collector server deployment

This directory is the image-only server deployment for the WeKnora replacement
architecture. The server does not compile source code.

## Local machine

```bash
./scripts/build-collector-bundle.sh weknora-collector
```

Upload `dist/images/aiks-collector-weknora-collector.tar` together with this
`deploy/server` directory to the server, and put the tar under `images/`.

## Server

First start aiks-WeKnora. Create the server-side WeKnora platform API key and
fill `AIKS_WEKNORA_API_KEY` in `.env`.

```bash
cp .env.example .env
vi .env
chmod +x start.sh
./start.sh
```

`start.sh` automatically loads every `images/*.tar[.gz]`, creates the shared
Docker network when needed, force-recreates the collector and waits for health.

The collector is bound to `127.0.0.1:28082` by default. Put HTTPS
Nginx/TongHttpServer in front of it for Desktop remote access.
