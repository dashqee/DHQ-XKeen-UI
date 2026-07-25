# DHQClash Mihomo external-ui

The `frontend` package produces a static Mihomo dashboard from the same DHQClash design system as DHQClash Router.

## Build

```bash
cd frontend
npm run build:external
```

The deployable directory is `frontend/dist-external`. Its contents must be placed at the root of the release ZIP, not inside an additional `dist-external` folder.

## Mihomo configuration

Merge [`mihomo.yaml`](./mihomo.yaml) into the client profile and replace `GENERATE_UNIQUE_SECRET` with a unique high-entropy value per router.

The dashboard opens at:

```text
http://<router-address>:9090/ui/
```

Keep the controller blocked from WAN. The static bundle never contains the API secret; the user enters it in the browser and it is retained only in `sessionStorage`.

Router-only operations such as filesystem configuration editing, backups, service installation and binary updates remain in the Axum-powered DHQClash Router panel.

Полная инструкция по установке на Entware и безопасной выдаче клиентской конфигурации находится в [`INSTALL_ENTWARE.md`](../INSTALL_ENTWARE.md).
