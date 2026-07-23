# Release Mirrors

Rainy installers can use any HTTPS static object store or CDN without calling
the GitHub API. A mirror uses this layout:

```text
rainy-cli/
  install.sh
  install.ps1
  latest.txt
  v0.3.5/
    rainy-x86_64-unknown-linux-gnu.tar.gz
    rainy-x86_64-unknown-linux-gnu.tar.gz.sha256
    rainy-aarch64-unknown-linux-gnu.tar.gz
    rainy-aarch64-unknown-linux-gnu.tar.gz.sha256
    rainy-x86_64-apple-darwin.tar.gz
    rainy-x86_64-apple-darwin.tar.gz.sha256
    rainy-aarch64-apple-darwin.tar.gz
    rainy-aarch64-apple-darwin.tar.gz.sha256
    rainy-x86_64-pc-windows-msvc.zip
    rainy-x86_64-pc-windows-msvc.zip.sha256
```

`latest.txt` contains one exact tag such as `v0.3.5`. Version directories are
immutable. Upload all versioned files first, then replace the root installers,
and update `latest.txt` last.

## Install From A Mirror

macOS or Linux:

```bash
MIRROR=https://downloads.example.com/rainy-cli
curl -fsSL "$MIRROR/install.sh" \
  | env RAINY_RELEASE_BASE_URL="$MIRROR" sh
```

Windows PowerShell:

```powershell
$env:RAINY_RELEASE_BASE_URL = "https://downloads.example.com/rainy-cli"
irm "$env:RAINY_RELEASE_BASE_URL/install.ps1" | iex
```

`RAINY_RELEASE_BASE_URL` makes `latest` resolve from `<mirror>/latest.txt` and
downloads assets from `<mirror>/<tag>/`. It does not contact GitHub. Use
`RAINY_LATEST_VERSION_URL` when the latest marker lives elsewhere, or
`RAINY_INSTALLER_BASE_URL` to point directly at one exact version directory.
When installation uses `RAINY_RELEASE_BASE_URL`, the installer stores the
validated, non-secret URL in `~/.rainy/release-source`. Subsequent
`rainy self check` and `rainy self update` automatically reuse that mirror.
An environment variable has priority over the saved source:

```bash
export RAINY_RELEASE_BASE_URL=https://downloads.example.com/rainy-cli
rainy self check
rainy self update
```

Delete `~/.rainy/release-source` and unset the environment variables to return
to the default GitHub release source.

## Publish To Alibaba Cloud OSS

Install `ossutil` and configure a RAM identity with write access to only the
chosen bucket prefix. Prefer environment variables or a protected config file;
do not put access keys in this repository.

```bash
export OSS_ACCESS_KEY_ID=...
export OSS_ACCESS_KEY_SECRET=...
export OSS_REGION=cn-hangzhou
export OSS_ENDPOINT=https://oss-cn-hangzhou.aliyuncs.com
```

Download or build the complete release assets into `dist`, then publish them:

```bash
gh release download v0.3.5 --dir dist
RAINY_OSS_DEST=oss://example-bucket/rainy-cli \
  sh scripts/publish-oss.sh v0.3.5 dist
```

Expose the prefix through an HTTPS OSS custom domain or CDN, for example
`https://downloads.example.com/rainy-cli`. The download endpoint must permit
unauthenticated reads, or the CDN must authenticate to a private OSS origin.
Keep `latest.txt` and the root installers on a short or no-cache policy.
Versioned archives can use a long, immutable cache policy.

The publisher validates the required platform archives and checksum files. It
uploads the immutable version directory first and changes `latest.txt` last, so
an interrupted upload does not advertise an incomplete release.
