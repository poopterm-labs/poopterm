# >>> poopterm shell integration >>>
# Source this file from ~/.zshrc to enable OSC 133 and OSC 7 integration.
if (( $+functions[_poopterm_precmd] )); then
	return
fi

autoload -Uz add-zsh-hook add-zle-hook-widget

_poopterm_precmd() {
	local status=$?
	print -rn -- $'\e]133;D;'"$status"$'\a'
	print -rn -- $'\e]133;A\a'
	local path=${PWD//\%/%25}
	path=${path// /%20}
	path=${path//#/%23}
	path=${path//\?/%3F}
	path=${path//\[/%5B}
	path=${path//\]/%5D}
	print -rn -- $'\e]7;file://'"$path"$'\a'
}

_poopterm_preexec() {
	print -rn -- $'\e]133;C\a'
}

_poopterm_line_init() {
	print -rn -- $'\e]133;B\a'
}

add-zsh-hook precmd _poopterm_precmd
add-zsh-hook preexec _poopterm_preexec
add-zle-hook-widget zle-line-init _poopterm_line_init
# <<< poopterm shell integration <<<
