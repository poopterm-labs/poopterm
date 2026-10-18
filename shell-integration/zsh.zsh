# >>> poopterm shell integration >>>
# Source this file from ~/.zshrc to enable OSC 133 and OSC 7 integration.
if (( $+functions[_poopterm_precmd] )); then
	return
fi

autoload -Uz add-zsh-hook add-zle-hook-widget

_poopterm_precmd() {
	# `status` is read-only and `path` is tied to $PATH in zsh; use other names.
	local exit_code=$?
	print -rn -- $'\e]133;D;'"$exit_code"$'\a'
	print -rn -- $'\e]133;A\a'
	local cwd=${PWD//\%/%25}
	cwd=${cwd// /%20}
	cwd=${cwd//\#/%23}
	cwd=${cwd//\?/%3F}
	cwd=${cwd//\[/%5B}
	cwd=${cwd//\]/%5D}
	print -rn -- $'\e]7;file://'"$cwd"$'\a'
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
