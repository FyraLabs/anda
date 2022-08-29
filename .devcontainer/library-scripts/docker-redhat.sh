#!/usr/bin/env bash
#-------------------------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See https://go.microsoft.com/fwlink/?linkid=2090316 for license information.
#-------------------------------------------------------------------------------------------------------------
#
# ** This script is community supported **
# Docs: https://github.com/microsoft/vscode-dev-containers/blob/main/script-library/docs/docker.md
# Maintainer: @smankoo
#
# Syntax: ./docker-redhat.sh [enable non-root docker socket access flag] [source socket] [target socket] [non-root user]

ENABLE_NONROOT_DOCKER=${1:-"true"}
SOURCE_SOCKET=${2:-"/var/run/docker-host.sock"}
TARGET_SOCKET=${3:-"/var/run/docker.sock"}
USERNAME=${4:-"automatic"}
DOCKER_DASH_COMPOSE_VERSION="1"

set -e

if [ "$(id -u)" -ne 0 ]; then
    echo -e 'Script must be run as root. Use sudo, su, or add "USER root" to your Dockerfile before running this script.'
    exit 1
fi

# Figure out correct version of a three part version number is not passed
find_version_from_git_tags() {
    local variable_name=$1
    local requested_version=${!variable_name}
    if [ "${requested_version}" = "none" ]; then return; fi
    local repository=$2
    local prefix=${3:-"tags/v"}
    local separator=${4:-"."}
    local last_part_optional=${5:-"false"}    
    if [ "$(echo "${requested_version}" | grep -o "." | wc -l)" != "2" ]; then
        local escaped_separator=${separator//./\\.}
        local last_part
        if [ "${last_part_optional}" = "true" ]; then
            last_part="(${escaped_separator}[0-9]+)?"
        else
            last_part="${escaped_separator}[0-9]+"
        fi
        local regex="${prefix}\\K[0-9]+${escaped_separator}[0-9]+${last_part}$"
        local version_list="$(git ls-remote --tags ${repository} | grep -oP "${regex}" | tr -d ' ' | tr "${separator}" "." | sort -rV)"
        if [ "${requested_version}" = "latest" ] || [ "${requested_version}" = "current" ] || [ "${requested_version}" = "lts" ]; then
            declare -g ${variable_name}="$(echo "${version_list}" | head -n 1)"
        else
            set +e
            declare -g ${variable_name}="$(echo "${version_list}" | grep -E -m 1 "^${requested_version//./\\.}([\\.\\s]|$)")"
            set -e
        fi
    fi
    if [ -z "${!variable_name}" ] || ! echo "${version_list}" | grep "^${!variable_name//./\\.}$" > /dev/null 2>&1; then
        echo -e "Invalid ${variable_name} value: ${requested_version}\nValid values:\n${version_list}" >&2
        exit 1
    fi
    echo "${variable_name}=${!variable_name}"
}

# Determine the appropriate non-root user
if [ "${USERNAME}" = "auto" ] || [ "${USERNAME}" = "automatic" ]; then
    USERNAME=""
    POSSIBLE_USERS=("vscode" "node" "codespace" "$(awk -v val=1000 -F ":" '$3==val{print $1}' /etc/passwd)")
    for CURRENT_USER in ${POSSIBLE_USERS[@]}; do
        if id -u ${CURRENT_USER} > /dev/null 2>&1; then
            USERNAME=${CURRENT_USER}
            break
        fi
    done
    if [ "${USERNAME}" = "" ]; then
        USERNAME=root
    fi
elif [ "${USERNAME}" = "none" ] || ! id -u ${USERNAME} > /dev/null 2>&1; then
    USERNAME=root
fi

# Install Prerequisites
if yum list deltarpm > /dev/null 2>&1; then
    yum -y install deltarpm
fi
yum -y install ca-certificates curl gnupg2 dirmngr dnf net-tools dialog git openssh-clients curl less procps 


# Try to load os-release
. /etc/os-release 2>/dev/null

# If unable to load OS Name and Verstion from os-release, install lsb_release
if [ $? -ne 0 ] || [ "${NAME}" = "" ] || [ "${VERSION_ID}" = "" ]; then

    yum -y install redhat-lsb-core

    OSNAME=$(lsb_release -is | tr '[:upper:]' '[:lower:]')
    RHEL_COMPAT_VER=${VERSION_ID:-`lsb_release -rs | cut -d. -f1`}

else
    OSNAME=`echo $NAME | cut -d" " -f1 | tr '[:upper:]' '[:lower:]'`
    if [ "${OSNAME}" = "amazon" ]; then
        if [ "${VERSION_ID}" = "2" ]; then
            RHEL_COMPAT_VER=7
        else
            echo "Incompatible Operative System. Exiting..."
            exit
        fi
    else
        RHEL_COMPAT_VER=${VERSION_ID%%.*}
    fi
fi

#yum install -y https://dl.fedoraproject.org/pub/epel/epel-release-latest-${RHEL_COMPAT_VER}.noarch.rpm
yum install -y yum-utils device-mapper-persistent-data lvm2
#yum-config-manager --add-repo https://download.docker.com/linux/centos/docker-ce.repo

yum -y update
# Install moby engine
dnf install -y moby-engine

# Install Docker Compose
dnf install docker-compose -y

# If init file already exists, exit
if [ -f "/usr/local/share/docker-init.sh" ]; then
    echo "/usr/local/share/docker-init.sh already exists, so exiting."
    exit 0
fi
echo "docker-init doesnt exist, adding..."

# Add user to the docker group
if [ "${ENABLE_NONROOT_DOCKER}" = "true" ]; then
    if ! getent group docker > /dev/null 2>&1; then
        groupadd docker
    fi

    usermod -aG docker ${USERNAME}
fi

tee /usr/local/share/docker-init.sh > /dev/null \
<< 'EOF'
#!/bin/sh
#-------------------------------------------------------------------------------------------------------------
# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License. See https://go.microsoft.com/fwlink/?linkid=2090316 for license information.
#-------------------------------------------------------------------------------------------------------------
set -e
dockerd_start="$(cat << 'INNEREOF'
    # explicitly remove dockerd and containerd PID file to ensure that it can start properly if it was stopped uncleanly
    # ie: docker kill <ID>
    find /run /var/run -iname 'docker*.pid' -delete || :
    find /run /var/run -iname 'container*.pid' -delete || :
    ## Dind wrapper script from docker team, adapted to a function
    # Maintained: https://github.com/moby/moby/blob/master/hack/dind
    export container=docker
    if [ -d /sys/kernel/security ] && ! mountpoint -q /sys/kernel/security; then
        mount -t securityfs none /sys/kernel/security || {
            echo >&2 'Could not mount /sys/kernel/security.'
            echo >&2 'AppArmor detection and --privileged mode might break.'
        }
    fi
    # Mount /tmp (conditionally)
    if ! mountpoint -q /tmp; then
        mount -t tmpfs none /tmp
    fi
    # cgroup v2: enable nesting
    if [ -f /sys/fs/cgroup/cgroup.controllers ]; then
        # move the processes from the root group to the /init group,
        # otherwise writing subtree_control fails with EBUSY.
        # An error during moving non-existent process (i.e., "cat") is ignored.
        mkdir -p /sys/fs/cgroup/init
        xargs -rn1 < /sys/fs/cgroup/cgroup.procs > /sys/fs/cgroup/init/cgroup.procs || :
        # enable controllers
        sed -e 's/ / +/g' -e 's/^/+/' < /sys/fs/cgroup/cgroup.controllers \
            > /sys/fs/cgroup/cgroup.subtree_control
    fi
    ## Dind wrapper over.
    # Handle DNS
    set +e
    cat /etc/resolv.conf | grep -i 'internal.cloudapp.net'
    if [ $? -eq 0 ]
    then
        echo "Setting dockerd Azure DNS."
        CUSTOMDNS="--dns 1.1.1.1"
    else
        echo "Not setting dockerd DNS manually."
        CUSTOMDNS=""
    fi
    set -e
    # Start docker/moby engine
    ( dockerd $CUSTOMDNS > /tmp/dockerd.log 2>&1 ) &
INNEREOF
)"
# Start using sudo if not invoked as root
if [ "$(id -u)" -ne 0 ]; then
    sudo /bin/sh -c "${dockerd_start}"
else
    eval "${dockerd_start}"
fi
set +e
# Execute whatever commands were passed in (if any). This allows us
# to set this script to ENTRYPOINT while still executing the default CMD.
exec "$@"
EOF

chmod +x /usr/local/share/docker-init.sh
chown ${USERNAME}:root /usr/local/share/docker-init.sh

echo 'docker-in-docker-fedora script has completed!'