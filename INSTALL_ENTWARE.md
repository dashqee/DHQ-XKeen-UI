# Установка DHQClash Router на Entware

Инструкция рассчитана на роутер с уже установленным и запускающимся Entware. Команды выполняются по SSH от `root`.

Поддерживаемые релизные архитектуры:

- `aarch64` → `arm64-v8a`;
- `mipsel` → `mips32le`;
- `mips` → `mips32`.

## 1. Проверка Entware

```sh
test -x /opt/bin/opkg && echo "Entware готов"
opkg print-architecture
df -h /opt
```

Убедитесь, что `/opt` смонтирован, архитектура входит в список выше, а на накопителе есть свободное место.

Обновите индекс пакетов и установите TLS-сертификаты и `curl`:

```sh
opkg update
opkg install ca-bundle ca-certificates curl
```

Для установки бета-версий дополнительно нужен `jq`:

```sh
opkg install jq
```

## 2. Установка DHQClash Router

Рекомендуемый вариант позволяет сначала просмотреть установщик:

```sh
cd /opt/tmp
curl -fL \
  https://raw.githubusercontent.com/dashqee/DHQ-XKeen-UI/main/setup.sh \
  -o setup-dhqclash.sh
sed -n '1,260p' setup-dhqclash.sh
sh setup-dhqclash.sh
```

В меню выберите `1. Установить/переустановить`.

Установщик:

- определит архитектуру через `opkg`;
- загрузит `dhqclash-router-<архитектура>`;
- установит бинарник как `/opt/sbin/xkeen-ui`;
- создаст `/opt/etc/init.d/S99xkeen-ui`;
- запустит панель на TCP-порту `1000`.

Короткий вариант:

```sh
curl -fL https://raw.githubusercontent.com/dashqee/DHQ-XKeen-UI/main/setup.sh | sh
```

Для pre-release:

```sh
curl -fL https://raw.githubusercontent.com/dashqee/DHQ-XKeen-UI/main/setup.sh | sh -s -- beta
```

## 3. Первый вход

Откройте из локальной сети:

```text
http://IP_РОУТЕРА:1000
```

IP роутера можно проверить командой:

```sh
ip -4 addr show br0
```

При первом запуске панель потребует создать пароль администратора. Используйте уникальный пароль и сохраните его в менеджере паролей.

Проверьте сервис:

```sh
/opt/etc/init.d/S99xkeen-ui status
pidof xkeen-ui
tail -n 100 /opt/var/log/xkeen-ui.log
```

Управление:

```sh
/opt/etc/init.d/S99xkeen-ui start
/opt/etc/init.d/S99xkeen-ui restart
/opt/etc/init.d/S99xkeen-ui stop
```

Entware запускает исполняемые скрипты `S*` из `/opt/etc/init.d`. Если панель не стартует после перезагрузки, сначала проверьте, что сам Entware и `/opt/etc/init.d/rc.unslung` запускаются прошивкой роутера.

## 4. Установка Mihomo external-ui

DHQClash выпускает интерфейс отдельным архивом:

```text
https://github.com/dashqee/DHQ-XKeen-UI/releases/latest/download/dhqclash-external-ui.zip
```

### Вариант A: загрузка средствами Mihomo

Объедините следующие параметры с `/opt/etc/mihomo/config.yaml`. Не создавайте дублирующиеся YAML-ключи, если `external-controller`, `secret`, `external-ui` или `profile` уже присутствуют.

```yaml
external-controller: 0.0.0.0:9090
secret: "УНИКАЛЬНЫЙ_СЕКРЕТ_ДЛЯ_ЭТОГО_РОУТЕРА"

external-ui: /opt/etc/mihomo/ui
external-ui-name: dhqclash
external-ui-url: "https://github.com/dashqee/DHQ-XKeen-UI/releases/latest/download/dhqclash-external-ui.zip"

profile:
  store-selected: true
  store-fake-ip: true
```

Перезапустите сервис, которым на устройстве управляется Mihomo. При установке через XKeen обычно используется:

```sh
/opt/etc/init.d/S99xkeen restart
```

### Вариант B: ручная установка архива

Этот способ подходит, если используемая сборка Mihomo не загружает `external-ui-url` автоматически:

```sh
opkg install unzip
mkdir -p /opt/etc/mihomo/ui
cd /opt/tmp
curl -fL \
  https://github.com/dashqee/DHQ-XKeen-UI/releases/latest/download/dhqclash-external-ui.zip \
  -o dhqclash-external-ui.zip
unzip -o dhqclash-external-ui.zip -d /opt/etc/mihomo/ui
test -f /opt/etc/mihomo/ui/index.html && echo "external-ui установлен"
```

После запуска Mihomo интерфейс доступен по адресу:

```text
http://IP_РОУТЕРА:9090/ui/
```

В форме подключения укажите секрет из `config.yaml`. Секрет не встраивается в интерфейс и хранится браузером только в `sessionStorage`.

## 5. Безопасность

- Не создавайте переадресацию портов `1000` и `9090` с WAN.
- Разрешайте доступ к ним только из домашней LAN или через собственный VPN.
- Для каждого клиентского роутера генерируйте отдельные пароль панели и `secret` Mihomo.
- Не кладите реальный `secret` в общий шаблон, мобильное приложение или публичный репозиторий.
- Передавайте индивидуальный секрет клиенту через защищённый канал либо создавайте его во время установки.
- После установки проверьте с внешней сети, что оба порта недоступны.

Панель и Mihomo API в текущей конфигурации слушают `0.0.0.0`, то есть все интерфейсы роутера. Это позволяет браузеру и мобильному приложению подключаться из домашней сети, но требует явного ограничения WAN-доступа firewall роутера.

## 6. Поставка вместе с мобильным приложением

Для каждого роутера комплект поставки должен содержать:

1. индивидуальный Mihomo-конфиг без общих паролей;
2. уникальный `secret`;
3. адрес роутера в локальной сети;
4. ссылку `http://IP_РОУТЕРА:9090/ui/`;
5. данные первого входа в `http://IP_РОУТЕРА:1000`;
6. версию установленного бинарника и конфигурации.

В мобильном приложении сохраняйте `secret` в системном защищённом хранилище. Не включайте его в QR-код или экспортируемый конфиг без явного предупреждения пользователю.

## 7. Обновление и удаление

Повторно запустите установщик и выберите:

- `2` — обновить;
- `3` — удалить панель.

Проверить установленную версию:

```sh
/opt/sbin/xkeen-ui -v
```

Удаление панели не является способом удалить или сбросить Mihomo-конфигурацию. Перед изменениями создайте резервную копию в расширенном режиме DHQClash Router.

## 8. Диагностика

Проверка прослушиваемых портов:

```sh
netstat -lntp 2>/dev/null | grep -E ':(1000|9090)[[:space:]]'
```

Проверка панели с самого роутера:

```sh
curl -I http://127.0.0.1:1000/
```

Проверка API Mihomo:

```sh
curl -H "Authorization: Bearer УНИКАЛЬНЫЙ_СЕКРЕТ" \
  http://127.0.0.1:9090/version
```

Основные пути:

| Назначение | Путь |
|---|---|
| Бинарник панели | `/opt/sbin/xkeen-ui` |
| Init-скрипт панели | `/opt/etc/init.d/S99xkeen-ui` |
| Настройки панели | `/opt/etc/xkeen/xkeen-ui.json` |
| Лог панели | `/opt/var/log/xkeen-ui.log` |
| Конфиг Mihomo | `/opt/etc/mihomo/config.yaml` |
| Mihomo external-ui | `/opt/etc/mihomo/ui` |
