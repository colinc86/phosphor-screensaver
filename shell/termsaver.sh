#!/usr/bin/env bash
# termsaver shell integration
# Source this file in your .bashrc, or preferably use: eval "$(termsaver init bash)"

# Guard: only run in interactive shells
[[ $- != *i* ]] && return

# Guard: don't activate inside nested shells if already active
[[ -n "$TERMSAVER_ACTIVE" ]] && return

# Read timeout from config, default 300 seconds
__termsaver_timeout() {
    local timeout
    timeout=$(termsaver print-timeout 2>/dev/null)
    echo "${timeout:-300}"
}

# Set TMOUT to trigger ALRM after idle period
TMOUT=$(__termsaver_timeout)

# Trap ALRM signal -- bash sends this when TMOUT expires at prompt
__termsaver_activate() {
    # Don't activate if stdin isn't a terminal
    [[ ! -t 0 ]] && return

    # Don't activate if another instance is running
    [[ -n "$TERMSAVER_ACTIVE" ]] && return

    export TERMSAVER_ACTIVE=1
    termsaver run
    unset TERMSAVER_ACTIVE
}

trap '__termsaver_activate' ALRM
