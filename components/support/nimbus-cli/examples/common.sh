#!/bin/bash
app_id=org.mozilla.ios.Fennec
dot_app=Client.app
# shellcheck disable=SC2012
app_dir=$(ls -d "$HOME/Library/Developer/Xcode/DerivedData/*/Build/Products/*-iphonesimulator/$dot_app" | head -n 1)

# Reset
Color_Off="\033[0m"       # Text Reset

# Regular Colors
# shellcheck disable=SC2034
Black="\033[30m"        # Black
# shellcheck disable=SC2034
Red="\033[31m"          # Red
Green="\033[32m"        # Green
Yellow="\033[33m"       # Yellow
# shellcheck disable=SC2034
Blue="\033[34m"         # Blue
# shellcheck disable=SC2034
Purple="\033[35m"       # Purple
Cyan="\033[36m"         # Cyan
# shellcheck disable=SC2034
White="\033[37m"        # White

display_prompt() {
  string="$*"
  echo -e "${Cyan}$ ${Color_Off}$string"
}

display_command() {
  string="$*"
  display_prompt "${Green}$string${Color_Off}"
}

display_comment() {
    string="$*"
    display_prompt "${Cyan}#${Color_Off} ${Yellow}$string${Color_Off}"
}

display_subcomment() {
    string="$*"
    display_comment "${Cyan}$string${Color_Off}"
}

display_break() {
  echo
  echo
  echo
}

anykey() {
    display_subcomment "…"
    read -n 1 -s -r
}

justrun() {
  cmd="$*"
  display_prompt "$cmd"
  eval "$cmd"
}

open_editor() {
  file="$*"
  justrun "code $file"
}

waitrun() {
  cmd="$*"
  display_command "$cmd"
  anykey
  eval "$cmd"
}

runwait() {
  cmd="$*"
  display_command "$cmd"
  eval "$cmd"
  anykey
}

app_terminate() {
  runwait "xcrun simctl terminate booted $app_id 2>/dev/null"
}

app_uninstall() {
  runwait "xcrun simctl uninstall booted $app_id 2>/dev/null"
}

app_install() {
  runwait "xcrun simctl install booted $app_dir"
}

app_reinstall() {
    display_subcomment "(On a device this is a manual step, but we automate this here for demo purposes)"
    app_terminate
    app_uninstall
    app_install
}

launch_safari() {
  justrun "xcrun simctl launch booted com.apple.mobilesafari"
}