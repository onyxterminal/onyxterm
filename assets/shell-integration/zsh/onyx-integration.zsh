typeset -gi _onyx_command_running=0
typeset -g _onyx_input_mark=$'%{\e]133;B\a%}'

_onyx_precmd() {
  local exit_code=$?
  if (( _onyx_command_running )); then
    builtin printf '\e]133;D;%d\a' $exit_code
    _onyx_command_running=0
  fi
  builtin printf '\e]133;A\a'
  precmd_functions=(${precmd_functions:#_onyx_mark_input} _onyx_mark_input)
}

_onyx_mark_input() {
  [[ $PS1 == *$_onyx_input_mark ]] || PS1=$PS1$_onyx_input_mark
}

_onyx_preexec() {
  _onyx_command_running=1
  builtin printf '\e]133;C;cmdline=%s\a' "${1//[[:cntrl:]]/ }"
}

autoload -Uz add-zsh-hook
add-zsh-hook precmd _onyx_precmd
add-zsh-hook precmd _onyx_mark_input
add-zsh-hook preexec _onyx_preexec
