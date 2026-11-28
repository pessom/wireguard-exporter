# WireGuard in a Docker container

When WireGuard runs in a container, its interface (`wg0`) lives in that
container's network namespace, not on the host. The exporter only sees
interfaces in its own network namespace, so it has to **join the WireGuard
container's network**:

- Docker Compose: `network_mode: service:<wireguard service>`;
- `docker run`: `--network container:<wireguard container>`.

The exporter still needs only `NET_ADMIN`; it does not need the WireGuard
configs or private keys.

Recipes below:

- [linuxserver/wireguard](#linuxserverwireguard), with peer names generated
  from its peer folders;
- [wg-easy](#wg-easy), which has its own Prometheus metrics, and when the
  exporter is still worth adding.

## linuxserver/wireguard

[linuxserver/wireguard](https://github.com/linuxserver/docker-wireguard)
does not export metrics itself. In server mode with named peers
(`PEERS=laptop,phone`) it creates one folder per peer, `peer_<name>`, with
the peer's public key in `publickey-peer_<name>`; with a number
(`PEERS=3`) the folders are `peer1`, `peer2`, … These folders are enough to
build the exporter's [peer names file](configuration.md#peer-names-file).

### Compose

The exporter is added to the same `compose.yaml` as WireGuard. Because it
shares the `wireguard` service's network, its port is published on that
service.

```yaml
services:
  wireguard:
    image: lscr.io/linuxserver/wireguard:latest
    container_name: wireguard
    cap_add:
      - NET_ADMIN
      - SYS_MODULE
    environment:
      - PUID=1000
      - PGID=1000
      - TZ=Etc/UTC
      - SERVERURL=vpn.example.com
      - PEERS=laptop,phone
    volumes:
      - ./wireguard:/config
      - /lib/modules:/lib/modules
    ports:
      - 51820:51820/udp
      # The exporter shares this service's network, so its port is published here.
      - 127.0.0.1:9586:9586/tcp
    sysctls:
      - net.ipv4.conf.all.src_valid_mark=1
    restart: unless-stopped

  wireguard-exporter:
    image: ghcr.io/pessom/wireguard-exporter:0.3.2
    network_mode: service:wireguard
    depends_on:
      - wireguard
    cap_drop: [ALL]
    cap_add: [NET_ADMIN]
    security_opt: ["no-new-privileges:true"]
    read_only: true
    volumes:
      - ./wireguard-exporter:/etc/wireguard_exporter:ro
    command: ["--wireguard.peer-names-file", "/etc/wireguard_exporter/peers.yml"]
    restart: unless-stopped
```

### Peer names

The exporter stops at startup if the names file is missing, so create it
after WireGuard has generated the peers and before starting the exporter:

```sh
docker compose up -d wireguard

mkdir -p wireguard-exporter
for dir in wireguard/peer*/; do
  id=$(basename "$dir")                                 # peer_laptop or peer1
  key=$(cat "$dir/publickey-$id") || continue
  printf '"%s": %s\n' "$key" "${id#peer_}"             # name: laptop or peer1
done > wireguard-exporter/peers.yml
chmod 0644 wireguard-exporter/peers.yml

docker compose up -d
```

Result:

```yaml
"xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=": laptop
"HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=": phone
```

- If the peer folders are not readable by your user, run the loop with
  `sudo sh -c '...'`. Only public keys are read.
- After changing `PEERS`, run the loop again. The exporter reloads the file
  on the next scrape; no restart is needed. The whole directory is mounted,
  so replacing the file is picked up too.
- You can add extra labels (site, owner) by editing the file by hand, see
  [peer names file](configuration.md#peer-names-file).

### Scraping

- Prometheus on the host: target `127.0.0.1:9586`.
- Prometheus in the same Compose project: target `wireguard:9586` (and drop
  the `ports` entry for `9586`).

## wg-easy

[wg-easy](https://github.com/wg-easy/wg-easy) has its own Prometheus endpoint
(`/metrics/prometheus`, enabled in *Admin Panel → General*) with per-client
traffic, last handshake and endpoint, labelled with the client name. For a
basic dashboard that is enough.

Adding the exporter makes sense when you want:

- metrics that keep working when the wg-easy web app is down or restarting;
- keepalive, preshared key and allowed IP metrics, and the
  [alerting rules](prometheus.md) and
  [Grafana dashboard](https://grafana.com/grafana/dashboards/26131-wireguard/)
  that come with this exporter;
- the same metrics for wg-easy and for WireGuard on other hosts.

**Kernel WireGuard only.** wg-easy creates a kernel WireGuard interface when
the host has the module, which is the usual case. Otherwise it falls back to
userspace `wireguard-go`, and with `EXPERIMENTAL_AWG` (planned as the default
in wg-easy 16) it may use AmneziaWG. The exporter reads only kernel WireGuard
and sees neither of the other two. Check which one you have:

```sh
docker exec wg-easy ip -d link show wg0 | grep -o 'wireguard\|amneziawg\|tun'
```

`wireguard` works with the exporter; `amneziawg` or `tun` (userspace) does
not. To keep standard WireGuard when `EXPERIMENTAL_AWG` is on, set
`OVERRIDE_AUTO_AWG=wg`.

Add the exporter to wg-easy's `docker-compose.yml`:

```yaml
services:
  wg-easy:
    # ... unchanged ...
    ports:
      - "51820:51820/udp"
      - "51821:51821/tcp"
      - "127.0.0.1:9586:9586/tcp"   # wireguard-exporter

  wireguard-exporter:
    image: ghcr.io/pessom/wireguard-exporter:0.3.2
    network_mode: service:wg-easy
    depends_on:
      - wg-easy
    cap_drop: [ALL]
    cap_add: [NET_ADMIN]
    security_opt: ["no-new-privileges:true"]
    read_only: true
    restart: unless-stopped
```

wg-easy keeps client names in its database, not in files, so the exporter
does not pick them up. Either write a
[peer names file](configuration.md#peer-names-file) by hand (list the public
keys with `docker exec wg-easy wg show wg0 peers`), or rely on wg-easy's own
metrics for names.

## Notes

- **Recreating the WireGuard container** (new image, changed settings) gives
  it a new network namespace, and the exporter has to follow. If the
  exporter stops showing the interface after an update, recreate it:
  `docker compose up -d --force-recreate wireguard-exporter` (with plain
  `docker run`, remove and start it again).
- **Client mode** (the container connects to a remote server and routes
  everything through the tunnel): replies to the published port may leave
  through the tunnel. Exclude your local networks from the tunnel as
  described in the
  [linuxserver/wireguard README](https://github.com/linuxserver/docker-wireguard#maintaining-local-access-to-attached-services),
  or scrape the exporter from a container in the same Compose project.
- **Rootless Docker/Podman** works too: both containers are in the same user
  namespace. See [rootless runtimes](installation.md#rootless-runtimes).
