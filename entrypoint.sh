#!/bin/sh

set -e
set -x

envsubst < /etc/config/Switchroom.toml.in > Switchroom.toml

exec switchroom "$@"
