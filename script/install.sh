#!/bin/bash

RED_COLOR='\e[1;31m'
GREEN_COLOR='\e[1;32m'
YELLOW_COLOR='\e[1;33m'
BLUE_COLOR='\e[1;34m'
PINK_COLOR='\e[1;35m'
SHAN='\e[1;33;5m'
RES='\e[0m'

HELP() {
  echo -e "\r\n${GREEN_COLOR}SDWAN Installation Script Help${RES}\r\n"
  echo "Usage: ./install.sh [command] [options]"
  echo
  echo "Commands:"
  echo "  install    Install SDWAN"
  echo "  uninstall  Uninstall SDWAN"
  echo "  update     Update SDWAN to the latest version"
  echo "  help       Show this help message"
  echo
  echo "Options:"
  echo "  --skip-folder-verify  Skip folder verification during installation"
  echo "  --skip-folder-fix     Skip automatic folder path fixing"
  echo "  --release-url URL    Set self-hosted release channel base URL"
  echo "  --gh-proxy URL       Optional URL prefix proxy for downloads"
  echo
  echo "Examples:"
  echo "  ./install.sh install /opt/sdwan"
  echo "  ./install.sh install --skip-folder-verify"
  echo "  ./install.sh install --release-url https://github.com/zzxym/sdwan"
  echo "  ./install.sh update --release-url https://github.com/zzxym/sdwan"
  echo "  SDWAN_RELEASE_BASE_URL=https://github.com/zzxym/sdwan ./install.sh update"
  echo "  ./install.sh update"
  echo "  ./install.sh uninstall"
}

# Show help if no arguments or help command is used
if [ $# -eq 0 ] || [ "$1" = "help" ]; then
  HELP
  exit 0
fi

# This script copy from alist , Thank for it!

SKIP_FOLDER_VERIFY=false
SKIP_FOLDER_FIX=false
NO_GH_PROXY=false
# Self-hosted release channel root; set via SDWAN_RELEASE_BASE_URL or --release-url.
RELEASE_BASE_URL="${SDWAN_RELEASE_BASE_URL:-}"
# Optional token for private GitHub repositories.
RELEASE_TOKEN="${SDWAN_RELEASE_TOKEN:-${GITHUB_TOKEN:-}}"
# Optional download URL prefix proxy; disabled by default.
GH_PROXY=''

COMMEND=$1
shift

# Check path
if [[ "$#" -ge 1 && ! "$1" == --* ]]; then
    INSTALL_PATH=$1
    shift
fi

# Check other option
while [[ "$#" -gt 0 ]]; do
    case $1 in
        --skip-folder-verify) SKIP_FOLDER_VERIFY=true ;;
        --skip-folder-fix) SKIP_FOLDER_FIX=true ;;
        --no-gh-proxy) NO_GH_PROXY=true ;;
        --release-url) if [ -n "$2" ]; then RELEASE_BASE_URL=$2; shift; else echo "Error: --release-url requires a URL"; exit 1; fi ;;
        --gh-proxy) 
            if [ -n "$2" ]; then
                GH_PROXY=$2
                shift
            else
                echo "Error: --gh-proxy requires a URL"
                exit 1
            fi
            ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
    shift
done

if [ -z "$INSTALL_PATH" ]; then
    INSTALL_PATH='/opt/sdwan'
fi

if [[ "$INSTALL_PATH" == */ ]]; then
    INSTALL_PATH=${INSTALL_PATH%?}
fi

if ! $SKIP_FOLDER_FIX && ! [[ "$INSTALL_PATH" == */sdwan ]]; then
    INSTALL_PATH="$INSTALL_PATH/sdwan"
fi

echo INSTALL PATH : $INSTALL_PATH
echo SKIP FOLDER FIX : $SKIP_FOLDER_FIX
echo SKIP FOLDER VERIFY : $SKIP_FOLDER_VERIFY

# clear

# check if unzip is installed
if ! command -v unzip >/dev/null 2>&1; then
  echo -e "\r\n${RED_COLOR}Error: unzip is not installed${RES}\r\n"
  exit 1
fi

# check if curl is installed
if ! command -v curl >/dev/null 2>&1; then
  echo -e "\r\n${RED_COLOR}Error: curl is not installed${RES}\r\n"
  exit 1
fi

echo -e "\r\n${RED_COLOR}----------------------NOTICE----------------------${RES}\r\n"
echo " This is a temporary script to install SDWAN "
echo " SDWAN requires a dedicated empty folder to install"
echo " SDWAN is a developing product and may have some issues "
echo " Using SDWAN requires some basic skills "
echo " You need to face the risks brought by using SDWAN at your own risk "
echo -e "\r\n${RED_COLOR}-------------------------------------------------${RES}\r\n"

# Get platform
if command -v arch >/dev/null 2>&1; then
  platform=$(arch)
else
  platform=$(uname -m)
fi

case "$platform" in
  amd64 | x86_64)
    ARCH="x86_64"
    ;;
  arm64 | aarch64 | *armv8*)
    ARCH="aarch64"
    ;;
  *armv7*)
    ARCH="armv7"
    ;;
  *arm*)
    ARCH="arm"
    ;;
  mips)
    ARCH="mips"
    ;;
  mipsel)
    ARCH="mipsel"
    ;;
  *)
    ARCH="UNKNOWN"
    ;;
esac

# support hf
if [[ "$ARCH" == "armv7" || "$ARCH" == "arm" ]]; then
  if cat /proc/cpuinfo | grep Features | grep -i 'half' >/dev/null 2>&1; then
    ARCH=${ARCH}hf
  fi
fi

echo -e "\r\n${GREEN_COLOR}Your platform: ${ARCH} (${platform}) ${RES}\r\n" 1>&2

if [ "$(id -u)" != "0" ]; then
  echo -e "\r\n${RED_COLOR}This script requires run as Root !${RES}\r\n" 1>&2
  exit 1
elif [ "$ARCH" == "UNKNOWN" ]; then
  echo -e "\r\n${RED_COLOR}Opus${RES}, this script do not support your platform\r\nTry ${GREEN_COLOR}install by hand${RES}\r\n"
  exit 1
fi

# Detect init system
if command -v systemctl >/dev/null 2>&1; then
  INIT_SYSTEM="systemd"
elif command -v rc-update >/dev/null 2>&1; then
  INIT_SYSTEM="openrc"
else
  echo -e "\r\n${RED_COLOR}Error: Unsupported init system (neither systemd nor OpenRC found)${RES}\r\n"
  exit 1
fi


RESOLVE_CHANNEL() {
    RELEASE_BASE_URL=${RELEASE_BASE_URL%/}
    if [ -z "$RELEASE_BASE_URL" ]; then
        echo -e "${RED_COLOR}Error: self-hosted release channel is not configured.${RES}"
        echo "Set SDWAN_RELEASE_BASE_URL or pass --release-url <URL>."
        exit 1
    fi

    CURL_AUTH=()
    if [ -n "$RELEASE_TOKEN" ]; then
        CURL_AUTH=(-H "Authorization: Bearer $RELEASE_TOKEN")
    fi

    if [[ "$RELEASE_BASE_URL" =~ ^https://github[.]com/([^/]+)/([^/]+)/?$ ]]; then
        RELEASE_MODE="github"
        VERSION_URL="$RELEASE_BASE_URL/releases/latest/download/VERSION"
        ASSET_BASE="$RELEASE_BASE_URL/releases/download"
    else
        RELEASE_MODE="static"
        VERSION_URL="$RELEASE_BASE_URL/VERSION"
        ASSET_BASE="$RELEASE_BASE_URL"
    fi
    echo "RELEASE CHANNEL : $RELEASE_BASE_URL ($RELEASE_MODE)"
}

CHECK() {
  if ! $SKIP_FOLDER_VERIFY; then
    if [ -f "$INSTALL_PATH/sdwan-core" ]; then
      echo "There is SDWAN in $INSTALL_PATH. Please choose other path or use \"update\""
        echo -e "Or use Try ${GREEN_COLOR}--skip-folder-verify${RES} to skip"
      exit 0
    fi
  fi

  if [ ! -d "$INSTALL_PATH/" ]; then
    mkdir -p $INSTALL_PATH
  else
    # Check weather path is empty
    if ! $SKIP_FOLDER_VERIFY; then
      if [ -n "$(ls -A $INSTALL_PATH)" ]; then
        echo "SDWAN requires to be installed in an empty directory. Please choose a empty path"
        echo -e "Or use Try ${GREEN_COLOR}--skip-folder-verify${RES} to skip"
        echo -e "Current path: $INSTALL_PATH ( use ${GREEN_COLOR}--skip-folder-fix${RES} to disable folder fix )"
        exit 1
      fi
    fi
  fi
}

INSTALL() {
  # Get version number
  # Self-hosted channel: VERSION is a plain text tag, e.g. v2.6.4
  LATEST_VERSION=$(curl -fsSL "${CURL_AUTH[@]}" "${VERSION_URL}" | tr -d "[:space:]")

  if [ -z "$LATEST_VERSION" ]; then
    echo -e "\r\n${RED_COLOR}Opus${RES}, failure to get latest version. Check your internet\r\nOr try ${GREEN_COLOR}install by hand${RES}\r\n"
    exit 1
  fi

  # Download
  echo -e "\r\n${GREEN_COLOR}Downloading SDWAN $LATEST_VERSION ...${RES}"
  rm -rf /tmp/sdwan_tmp_install.zip
  BASE_URL="${ASSET_BASE}/${LATEST_VERSION}/sdwan-linux-${ARCH}-${LATEST_VERSION}.zip"
  DOWNLOAD_URL=$($NO_GH_PROXY && echo "$BASE_URL" || echo "${GH_PROXY}${BASE_URL}")
  echo -e "Download URL: ${GREEN_COLOR}${DOWNLOAD_URL}${RES}"
  curl -L "${CURL_AUTH[@]}" ${DOWNLOAD_URL} -o /tmp/sdwan_tmp_install.zip $CURL_BAR

  # Unzip resource
  echo -e "\r\n${GREEN_COLOR}Unzip resource ...${RES}"
  unzip -o /tmp/sdwan_tmp_install.zip -d $INSTALL_PATH/
  mkdir $INSTALL_PATH/config
  mv $INSTALL_PATH/sdwan-linux-${ARCH}/* $INSTALL_PATH/
  rm -rf $INSTALL_PATH/sdwan-linux-${ARCH}/
  chmod +x $INSTALL_PATH/sdwan-core $INSTALL_PATH/sdwan-cli
  if [ -f $INSTALL_PATH/sdwan-core ] || [ -f $INSTALL_PATH/sdwan-cli ]; then
    echo -e "${GREEN_COLOR} Download successfully! ${RES}"
  else
    echo -e "${RED_COLOR} Download failed! ${RES}"
    exit 1
  fi
}

INIT() {
  if [ ! -f "$INSTALL_PATH/sdwan-core" ]; then
    echo -e "\r\n${RED_COLOR}Opus${RES}, unable to find SDWAN\r\n"
    exit 1
  fi

  # Create default blank file config
  cat >$INSTALL_PATH/config/default.conf <<EOF
instance_name = "default"
dhcp = true
listeners = [
    "tcp://0.0.0.0:10010",
    "udp://0.0.0.0:10010",
    "wg://0.0.0.0:10011",
    "ws://0.0.0.0:10011/",
    "wss://0.0.0.0:10012/",
]
exit_nodes = []
rpc_portal = "0.0.0.0:0"

[[peer]]
uri = "tcp://public.sdwan.top:10010"

[network_identity]
network_name = "default"
network_secret = "default"

[flags]
default_protocol = "udp"
dev_name = ""
enable_encryption = true
enable_ipv6 = true
mtu = 1380
latency_first = false
enable_exit_node = false
no_tun = false
use_smoltcp = false
foreign_network_whitelist = "*"
disable_p2p = false
p2p_only = false
relay_all_peer_rpc = false
disable_tcp_hole_punching = false
disable_udp_hole_punching = false

EOF

  # Create init script
  if [ "$INIT_SYSTEM" = "openrc" ]; then
    cat >/etc/init.d/sdwan <<EOF
#!/sbin/openrc-run

name="SDWAN"
description="SDWAN Service"
command="$INSTALL_PATH/sdwan-core"
command_args="-c $INSTALL_PATH/config/default.conf"
command_user="nobody:nobody"
command_background=true

pidfile="/run/\${RC_SVCNAME}.pid"

depend() {
  need net
}


EOF
    chmod +x /etc/init.d/sdwan
  fi

  # Create systemd
  if [ "$INIT_SYSTEM" = "systemd" ]; then
    cat >/etc/systemd/system/sdwan@.service <<EOF
[Unit]
Description=SDWAN Service
Wants=network.target
After=network.target network.service
StartLimitIntervalSec=0

[Service]
Type=simple
WorkingDirectory=$INSTALL_PATH
ExecStart=$INSTALL_PATH/sdwan-core -c $INSTALL_PATH/config/%i.conf
Restart=always
RestartSec=1s

[Install]
WantedBy=multi-user.target
EOF
  fi

#   # Create run script
#   cat >$INSTALL_PATH/run.sh <<EOF
# $INSTALL_PATH/sdwan-core
# EOF

  # Startup
  if [ "$INIT_SYSTEM" = "systemd" ]; then
    systemctl daemon-reload
    systemctl enable sdwan@default >/dev/null 2>&1
    systemctl start sdwan@default
  else
    rc-update add sdwan default
    rc-service sdwan start
  fi

  # For issues from the previous version
  rm -rf /etc/systemd/system/sdwan.service
  rm -rf /usr/bin/sdwan-core
  rm -rf /usr/bin/sdwan-cli

  # Add link
  ln -s $INSTALL_PATH/sdwan-core /usr/sbin/sdwan-core
  ln -s $INSTALL_PATH/sdwan-cli /usr/sbin/sdwan-cli
}

SUCCESS() {
  clear
  echo " Install SDWAN successfully!"
  echo -e "\r\nDefault Port: ${GREEN_COLOR}10010(UDP+TCP)${RES}, Notice allowing in firewall!\r\n"
  echo -e "Default Network Name: ${GREEN_COLOR}default${RES}, Please change it to your own network name!\r\n"

  echo -e "Now SDWAN supports multiple config files. You can create config files in the ${GREEN_COLOR}${INSTALL_PATH}/config/${RES} folder"
  echo -e "For more information, please check the documents in official site"
  echo -e "The management example of a single configuration file is as follows"

  echo
  if [ "$INIT_SYSTEM" = "systemd" ]; then
    echo -e "Status: ${GREEN_COLOR}systemctl status sdwan@default${RES}"
    echo -e "Start: ${GREEN_COLOR}systemctl start sdwan@default${RES}"
    echo -e "Restart: ${GREEN_COLOR}systemctl restart sdwan@default${RES}"
    echo -e "Stop: ${GREEN_COLOR}systemctl stop sdwan@default${RES}"
  else
    echo -e "Status: ${GREEN_COLOR}rc-service sdwan status${RES}"
    echo -e "Start: ${GREEN_COLOR}rc-service sdwan start${RES}"
    echo -e "Restart: ${GREEN_COLOR}rc-service sdwan restart${RES}"
    echo -e "Stop: ${GREEN_COLOR}rc-service sdwan stop${RES}"
  fi
  echo
}

UNINSTALL() {
  echo -e "\r\n${GREEN_COLOR}Uninstall SDWAN ...${RES}\r\n"
  echo -e "${GREEN_COLOR}Stop process ...${RES}"
  if [ "$INIT_SYSTEM" = "systemd" ]; then
    systemctl disable "sdwan@*" >/dev/null 2>&1
    systemctl stop "sdwan@*" >/dev/null 2>&1
    # Also clean up services left by legacy sdwan installations.
    systemctl disable "sdwan@*" >/dev/null 2>&1
    systemctl stop "sdwan@*" >/dev/null 2>&1
  else
    rc-update del sdwan
    rc-service sdwan stop
  fi
  echo -e "${GREEN_COLOR}Delete files ...${RES}"
  if [ "$INIT_SYSTEM" = "systemd" ]; then
    rm -rf $INSTALL_PATH /etc/systemd/system/sdwan.service /usr/bin/sdwan-core /usr/bin/sdwan-cli /etc/systemd/system/sdwan@.service /usr/sbin/sdwan-core /usr/sbin/sdwan-cli
    rm -rf /etc/systemd/system/sdwan@.service /etc/init.d/sdwan
    systemctl daemon-reload
  else
    rm -rf $INSTALL_PATH /etc/init.d/sdwan /usr/bin/sdwan-core /usr/bin/sdwan-cli /usr/sbin/sdwan-core /usr/sbin/sdwan-cli
    rc-update del sdwan default >/dev/null 2>&1
    rm -rf /etc/init.d/sdwan
  fi
  echo -e "\r\n${GREEN_COLOR}SDWAN was removed successfully! ${RES}\r\n"
}

# Minimizes downtime by preparing new files before stopping the service.
# Correctly handles restarting multiple systemd service instances.
UPDATE() {
  if [ ! -f "$INSTALL_PATH/sdwan-core" ]; then
    echo -e "\r\n${RED_COLOR}Error${RES}: SDWAN not found in $INSTALL_PATH. Cannot perform update.\r\n"
    exit 1
  fi

  # 1. Get the latest version info (while service is still running)
  echo -e "${GREEN_COLOR}Checking for the latest version...${RES}"
  # Self-hosted channel: VERSION is a plain text tag, e.g. v2.6.4
  LATEST_VERSION=$(curl -fsSL "${CURL_AUTH[@]}" "${VERSION_URL}" | tr -d "[:space:]")

  if [ -z "$LATEST_VERSION" ]; then
    echo -e "\r\n${RED_COLOR}Error${RES}: Failed to get the latest version. Please check your network connection.\r\n"
    exit 1
  fi

  echo -e "Latest version found: ${GREEN_COLOR}$LATEST_VERSION${RES}"

  # 2. Download and extract the new version to a temporary directory (while service is still running)
  TEMP_UPDATE_DIR=$(mktemp -d /tmp/sdwan_update_XXXXXX)
  echo -e "${GREEN_COLOR}Downloading new version to temporary directory: $TEMP_UPDATE_DIR${RES}"
  
  BASE_URL="${ASSET_BASE}/${LATEST_VERSION}/sdwan-linux-${ARCH}-${LATEST_VERSION}.zip"
  DOWNLOAD_URL=$($NO_GH_PROXY && echo "$BASE_URL" || echo "${GH_PROXY}${BASE_URL}")
  
  echo -e "Download URL: ${GREEN_COLOR}${DOWNLOAD_URL}${RES}"
  curl -L "${CURL_AUTH[@]}" ${DOWNLOAD_URL} -o "$TEMP_UPDATE_DIR/sdwan.zip" $CURL_BAR
  if [ $? -ne 0 ]; then
      echo -e "${RED_COLOR}Download failed!${RES}"
      rm -rf "$TEMP_UPDATE_DIR"
      exit 1
  fi
  
  unzip -o "$TEMP_UPDATE_DIR/sdwan.zip" -d "$TEMP_UPDATE_DIR/"
  
  NEW_CORE_FILE="$TEMP_UPDATE_DIR/sdwan-linux-${ARCH}/sdwan-core"
  if [ ! -f "$NEW_CORE_FILE" ]; then
      echo -e "${RED_COLOR}Extraction failed or the downloaded archive is invalid.${RES}"
      rm -rf "$TEMP_UPDATE_DIR"
      exit 1
  fi
  
  echo -e "${GREEN_COLOR}New version is ready. Starting update process...${RES}"
  
  # 3. Enter minimal downtime window
  
  # Record currently running service instances before stopping them
  ACTIVE_SERVICES=()
  if [ "$INIT_SYSTEM" = "systemd" ]; then
    # Get the list of active instances and store them in an array
    mapfile -t ACTIVE_SERVICES < <(systemctl list-units --type=service --state=active | grep "sdwan@" | awk '{print $1}')
    if [ ${#ACTIVE_SERVICES[@]} -gt 0 ]; then
        echo -e "\r\n${YELLOW_COLOR}Found running services: ${ACTIVE_SERVICES[*]}${RES}"
        echo -e "${YELLOW_COLOR}Stopping SDWAN services...${RES}"
        systemctl stop "${ACTIVE_SERVICES[@]}"
    else
        echo -e "\r\n${YELLOW_COLOR}No running SDWAN services found. Nothing to stop.${RES}"
    fi
  else # openrc
    # openrc script seems to handle a single service, so keep it simple
    echo -e "\r\n${YELLOW_COLOR}Stopping SDWAN service...${RES}"
    rc-service sdwan stop
  fi

  # Backup critical files, primarily the configuration
  echo "Backing up configuration..."
  BACKUP_CONFIG_DIR=$(mktemp -d /tmp/sdwan_config_backup_XXXXXX)
  if [ -d "$INSTALL_PATH/config" ]; then
      cp -a "$INSTALL_PATH/config" "$BACKUP_CONFIG_DIR/"
  fi
  
  echo "Replacing files..."
  # Remove old binaries and docs, but not the config directory
  rm -f "$INSTALL_PATH/sdwan-core" "$INSTALL_PATH/sdwan-cli" "$INSTALL_PATH/LICENSE" "$INSTALL_PATH/README.md"
  
  # Move new files into the installation directory
  mv "$TEMP_UPDATE_DIR/sdwan-linux-${ARCH}"/* "$INSTALL_PATH/"
  chmod +x "$INSTALL_PATH/sdwan-core" "$INSTALL_PATH/sdwan-cli"

  # Restore configuration to prevent user-defined settings from being overwritten
  if [ -d "$BACKUP_CONFIG_DIR/config" ]; then
      cp -af "$BACKUP_CONFIG_DIR/config/." "$INSTALL_PATH/config/"
  fi
  
  # 4. Start the services, restoring operation
  if [ "$INIT_SYSTEM" = "systemd" ]; then
    if [ ${#ACTIVE_SERVICES[@]} -gt 0 ]; then
        echo -e "${GREEN_COLOR}Starting new version of SDWAN services: ${ACTIVE_SERVICES[*]}${RES}"
        systemctl start "${ACTIVE_SERVICES[@]}"
    else
        echo -e "${GREEN_COLOR}No services were running before the update. Update complete.${RES}"
    fi
  else # openrc
    echo -e "${GREEN_COLOR}Starting new version of SDWAN service...${RES}"
    rc-service sdwan start
  fi
  
  # 5. Clean up temporary files
  echo "Cleaning up temporary files..."
  rm -rf "$TEMP_UPDATE_DIR"
  rm -rf "$BACKUP_CONFIG_DIR"
  
  echo -e "\r\n${GREEN_COLOR}SDWAN was successfully updated to version $LATEST_VERSION!${RES}\r\n"
}


# CURL progress
if curl --help | grep progress-bar >/dev/null 2>&1; then # $CURL_BAR
  CURL_BAR="--progress-bar"
fi

# The temp directory must exist
if [ ! -d "/tmp" ]; then
  mkdir -p /tmp
fi

if [ "$COMMEND" = "install" ] || [ "$COMMEND" = "update" ]; then
    RESOLVE_CHANNEL
fi

echo $COMMEND

if [ "$COMMEND" = "uninstall" ]; then
  UNINSTALL
elif [ "$COMMEND" = "update" ]; then
  UPDATE
elif [ "$COMMEND" = "install" ]; then
  CHECK
  INSTALL
  INIT
  if [ -f "$INSTALL_PATH/sdwan-core" ]; then
    SUCCESS
  else
    echo -e "${RED_COLOR} Install fail, try install by hand${RES}"
  fi
else
  echo -e "${RED_COLOR} Error Command ${RES}\n\r"
  echo " ALLOW:"
  echo -e "\n\r${GREEN_COLOR} install, uninstall, update, help ${RES}"
fi

rm -rf /tmp/sdwan_tmp_*
