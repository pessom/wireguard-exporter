# Ansible

There is no published role or collection. This page is an example role to
copy into your own repository. It is written for `ansible-pull` and offline
use:

- only `ansible.builtin` modules: no collections, no `requirements.yml`;
- nothing is downloaded when the requested version is already installed;
- the archive is verified against `sha256sums.txt` of the release;
- the download URL can point to an internal mirror;
- the systemd unit comes **from the release archive**, so hardening
  improvements arrive with new versions; flags go to
  `/etc/default/wireguard_exporter`;
- every version lives in its own directory, `/usr/local/bin/wireguard_exporter`
  is a symlink to the active one, so switching versions (and rolling back) is
  a variable change;
- peer names are written to a file the exporter reloads itself: changing
  them does not restart the service.

Requires facts (`gather_facts: true`) and `ansible-core` ≥ 2.15.

## Layout

```
roles/wireguard-exporter/
├── defaults/main.yml
├── handlers/main.yml
└── tasks/main.yml
```

## defaults/main.yml

```yaml
---
wireguard_exporter_version: "0.3.0"
wireguard_exporter_install_path: /opt/wireguard_exporter
wireguard_exporter_listen_address: "0.0.0.0:9586"
# Empty: autodiscovery of all WireGuard interfaces.
wireguard_exporter_interfaces: []
# public key -> name, or public key -> {name: ..., extra_label: ...}
# Changes are picked up without a restart.
wireguard_exporter_peer_names: {}
# Privacy: export peer endpoints (public IPs) and allowed IPs.
wireguard_exporter_peer_endpoint: true
wireguard_exporter_peer_allowed_ips: true
wireguard_exporter_extra_args: []
# Keep only the active version directory.
wireguard_exporter_remove_old_versions: true

wireguard_exporter_arch: >-
  {{ {'x86_64': 'amd64', 'aarch64': 'arm64'}[ansible_facts['architecture']] }}
# Point to a mirror for offline installations (same file layout as GitHub).
wireguard_exporter_base_url: "https://github.com/pessom/wireguard-exporter/releases/download/v{{ wireguard_exporter_version }}"
wireguard_exporter_archive: "wireguard_exporter-{{ wireguard_exporter_version }}.linux-{{ wireguard_exporter_arch }}"
```

## tasks/main.yml

```yaml
---
# Each version in its own directory; the symlink selects the active one.
- name: Check installed wireguard_exporter version
  ansible.builtin.stat:
    path: "{{ wireguard_exporter_install_path }}/{{ wireguard_exporter_version }}/wireguard_exporter"
  register: _wireguard_exporter_bin

- name: Install wireguard_exporter {{ wireguard_exporter_version }}
  when: not _wireguard_exporter_bin.stat.exists
  block:
    - name: Create version directory
      ansible.builtin.file:
        path: "{{ wireguard_exporter_install_path }}/{{ wireguard_exporter_version }}"
        state: directory
        mode: "0755"

    - name: Download release archive (sha256 verified)
      ansible.builtin.get_url:
        url: "{{ wireguard_exporter_base_url }}/{{ wireguard_exporter_archive }}.tar.gz"
        checksum: "sha256:{{ wireguard_exporter_base_url }}/sha256sums.txt"
        dest: "/tmp/{{ wireguard_exporter_archive }}.tar.gz"
        mode: "0644"

    - name: Extract binary and systemd unit
      ansible.builtin.unarchive:
        src: "/tmp/{{ wireguard_exporter_archive }}.tar.gz"
        dest: "{{ wireguard_exporter_install_path }}/{{ wireguard_exporter_version }}"
        remote_src: true
        extra_opts: ["--strip-components=1"]

  always:
    - name: Remove downloaded archive
      ansible.builtin.file:
        path: "/tmp/{{ wireguard_exporter_archive }}.tar.gz"
        state: absent

- name: Activate version
  ansible.builtin.file:
    src: "{{ wireguard_exporter_install_path }}/{{ wireguard_exporter_version }}/wireguard_exporter"
    dest: /usr/local/bin/wireguard_exporter
    state: link
    force: true
  notify: Restart wireguard_exporter

- name: Install systemd unit from the release
  ansible.builtin.copy:
    src: "{{ wireguard_exporter_install_path }}/{{ wireguard_exporter_version }}/wireguard_exporter.service"
    dest: /etc/systemd/system/wireguard_exporter.service
    remote_src: true
    mode: "0644"
  notify: Restart wireguard_exporter

# Readable by the systemd dynamic user: directory 0755, file 0644 (no secrets).
- name: Peer names directory
  ansible.builtin.file:
    path: /etc/wireguard_exporter
    state: directory
    mode: "0755"

- name: Peer names file (reloaded by the exporter, no restart)
  ansible.builtin.copy:
    dest: /etc/wireguard_exporter/peers.yml
    content: |
      # {{ ansible_managed }}
      {{ wireguard_exporter_peer_names | to_nice_yaml if wireguard_exporter_peer_names else '' }}
    mode: "0644"

- name: Exporter flags
  ansible.builtin.copy:
    dest: /etc/default/wireguard_exporter
    content: |
      # {{ ansible_managed }}
      WG_EXPORTER_OPTS="{{ _wireguard_exporter_opts | join(' ') }}"
    mode: "0644"
  vars:
    _wireguard_exporter_opts: >-
      {{
        ['--web.listen-address=' ~ wireguard_exporter_listen_address,
         '--wireguard.peer-names-file=/etc/wireguard_exporter/peers.yml']
        + (['--wireguard.interfaces=' ~ (wireguard_exporter_interfaces | join(','))] if wireguard_exporter_interfaces else [])
        + ([] if wireguard_exporter_peer_endpoint else ['--no-collector.peer-endpoint'])
        + ([] if wireguard_exporter_peer_allowed_ips else ['--no-collector.peer-allowed-ips'])
        + wireguard_exporter_extra_args
      }}
  notify: Restart wireguard_exporter

- name: Enable and start wireguard_exporter
  ansible.builtin.systemd_service:
    name: wireguard_exporter
    state: started
    enabled: true
    daemon_reload: true

- name: Find old version directories
  when: wireguard_exporter_remove_old_versions
  ansible.builtin.find:
    paths: "{{ wireguard_exporter_install_path }}"
    file_type: directory
    excludes: ["{{ wireguard_exporter_version }}"]
  register: _wireguard_exporter_old

- name: Remove old version directories
  ansible.builtin.file:
    path: "{{ item.path }}"
    state: absent
  loop: "{{ _wireguard_exporter_old.files | default([]) }}"
  loop_control:
    label: "{{ item.path }}"

- name: Flush handlers
  ansible.builtin.meta: flush_handlers
```

## handlers/main.yml

```yaml
---
- name: Restart wireguard_exporter
  ansible.builtin.systemd_service:
    name: wireguard_exporter
    state: restarted
    daemon_reload: true
```

## Usage

```yaml
- hosts: vpn
  become: true
  roles:
    - role: wireguard-exporter
      vars:
        wireguard_exporter_listen_address: "10.0.0.1:9586"
        wireguard_exporter_peer_names:
          "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=": alice-laptop
          "HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=":
            name: office-gateway
            site: fra
```

Notes:

- Run the role with `become: true` (play or role level).
- Upgrading: change `wireguard_exporter_version`. Archives older than 0.3.1
  ship a less hardened unit; the role installs whatever the release contains.
- The exporter refuses to start if the peer names file is invalid. Validate
  changes before rolling them out, e.g. on a test host.
- Without internet access, mirror `<archive>.tar.gz` and `sha256sums.txt`
  and set `wireguard_exporter_base_url`.
