#!/bin/sh

version='v'$(grep '^version =' ../../sdwan/Cargo.toml | cut -d '"' -f 2)

if [ -z "$version" ]; then
    echo "Error: 版本号不存在."
    exit 1
fi

filename="sdwan_magisk_${version}.zip"
echo $version

if [ -f "./sdwan-core" ] && [ -f "./sdwan-cli" ] && [ -f "./sdwan-web" ]; then
    zip -r -o -X "$filename" ./ -x '.git/*' -x '.github/*' -x 'folder/*' -x 'build.sh' -x 'magisk_update.json'
else
    RELEASE_BASE_URL="${SDWAN_RELEASE_BASE_URL:-}"
    if [ -z "$RELEASE_BASE_URL" ]; then
        echo "Error: SDWAN_RELEASE_BASE_URL is not set."
        exit 1
    fi
    RELEASE_BASE_URL="${RELEASE_BASE_URL%/}"
    case "$RELEASE_BASE_URL" in
        https://github.com/*/*) ASSET_BASE="$RELEASE_BASE_URL/releases/download" ;;
        *) ASSET_BASE="$RELEASE_BASE_URL" ;;
    esac
    wget -O "sdwan_last.zip" "$ASSET_BASE/$version/sdwan-linux-aarch64-$version.zip"
    unzip -o sdwan_last.zip -d ./
    mv ./sdwan-linux-aarch64/* ./
    rm -rf ./sdwan_last.zip
    rm -rf ./sdwan-linux-aarch64
    zip -r -o -X "$filename" ./ -x '.git/*' -x '.github/*' -x 'folder/*' -x 'build.sh' -x 'magisk_update.json'
fi
