# miniOS
自作OS

## setup

```bash
mkdir -p third_party/ovmf
cd third_party/ovmf

# Linux/Windows (with wget)
wget https://github.com/hikalium/wasabi/raw/main/third_party/ovmf/RELEASEX64_OVMF.fd

# macOS (with curl)
curl -L -o RELEASEX64_OVMF.fd https://github.com/hikalium/wasabi/raw/main/third_party/ovmf/RELEASEX64_OVMF.fd

cd -
```
