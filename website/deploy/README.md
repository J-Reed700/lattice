# lattice.lambdaaudio.com deployment

The Lattice website runs next to `lambdaaudio.com`, `headroommusic.app`, `downsample.app`, and `joshreed.net` on the existing IONOS server (`root@74.208.91.124`) as an isolated read-only Nginx container on the `pangolin` Docker network.

## Production shape

- **Public URL:** `https://lattice.lambdaaudio.com/`
- **Server:** `root@74.208.91.124` (`pangolin-ionos`), SSH key `~/.ssh/id_ed25519_headroom_ionos`
- **Cloudflare DNS (`lambdaaudio.com` zone):**
  - Type `A` (or `CNAME` to `lambdaaudio.com`), Name `lattice`, Value `74.208.91.124`, Proxy status **DNS only** (grey cloud), TTL Auto
- **Pangolin (`PoshCity` / `LambdaAudio`, site `IONOS Local`):**
  - Public resource: `Lattice Website`
  - Domain: `lattice.lambdaaudio.com`
  - Target: `http://lattice-site:8080`
  - Platform SSO: **Off**; HTTPS and HTTP-to-HTTPS redirect **On** (Let's Encrypt)
- **Docker network:** external `pangolin` network; no published host ports
- **Server directories:** `/opt/lattice/releases/<release-tag>` and symlink `/opt/lattice/current`
- **Container hardening:** runs as UID `101:101`, `read_only: true`, `/tmp` tmpfs (`noexec,nosuid,nodev`), `cap_drop: ALL`

## Package and deploy a release

From the repository root:

```sh
python3 website/deploy/package_site.py target/website-release/lattice-next
cat target/website-release/lattice-next/.env
COPYFILE_DISABLE=1 tar --no-xattrs -czf target/website-release/lattice-next.tgz \
  -C target/website-release/lattice-next .
shasum -a 256 target/website-release/lattice-next.tgz
```

Upload to IONOS and deploy into a fresh release directory (replace `RELEASE_TAG` with the tag printed by `package_site.py`):

```sh
scp -i ~/.ssh/id_ed25519_headroom_ionos -o IdentitiesOnly=yes \
  target/website-release/lattice-next.tgz root@74.208.91.124:/opt/lattice/lattice-next.tgz

ssh -i ~/.ssh/id_ed25519_headroom_ionos -o IdentitiesOnly=yes root@74.208.91.124
```

On the server:

```sh
install -d -m 755 /opt/lattice/releases/RELEASE_TAG
tar -xzf /opt/lattice/lattice-next.tgz -C /opt/lattice/releases/RELEASE_TAG
rm -f /opt/lattice/lattice-next.tgz
cd /opt/lattice/releases/RELEASE_TAG
sha256sum -c SHA256SUMS
docker compose build
docker compose up -d --wait
docker compose exec -T lattice-site nginx -t </dev/null
docker compose exec -T lattice-site wget -qO- http://127.0.0.1:8080/healthz </dev/null
ln -sfn /opt/lattice/releases/RELEASE_TAG /opt/lattice/current
```

## Verify publicly

After creating the `lattice` DNS record in Cloudflare and the `lattice.lambdaaudio.com` public resource in Pangolin (`http://lattice-site:8080`):

```sh
curl -fsS https://lattice.lambdaaudio.com/healthz
curl -fsSI https://lattice.lambdaaudio.com/
curl -fsSI https://lattice.lambdaaudio.com/downloads/Lattice_1.0.0_aarch64.dmg
curl -fsS https://lattice.lambdaaudio.com/downloads/Lattice_1.0.0_aarch64.dmg.sha256
```

## Rollback

Keep previous release directories and images on `/opt/lattice/releases/`. To roll back:

```sh
cd /opt/lattice/releases/PREVIOUS_TAG
docker compose up -d --no-build
ln -sfn /opt/lattice/releases/PREVIOUS_TAG /opt/lattice/current
```
