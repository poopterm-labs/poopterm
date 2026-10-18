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
	# Percent-encode every byte outside the URI-unreserved set, so control
	# characters in directory names cannot end the OSC 7 sequence early.
	local LC_ALL=C cwd= char hex
	for char in ${(s::)PWD}; do
		case $char in
			([a-zA-Z0-9/._~-]) cwd+=$char ;;
			(*) printf -v hex '%%%02X' "'$char"; cwd+=$hex ;;
		esac
	done
	print -rn -- $'\e]7;file://'"$cwd"$'\a'
}

_poopterm_preexec() {
	print -rn -- $'\e]133;C\a'
}

_poopterm_line_init() {
	print -rn -- $'\e]133;B\a'
}

# Remote hosts rarely have poopterm's terminfo entry; connect with a common
# TERM instead. Set POOPTERM_SSH_KEEP_TERM=1 to keep xterm-poopterm.
if [[ $TERM == xterm-poopterm && -z ${POOPTERM_SSH_KEEP_TERM-} ]] && (( ! $+functions[ssh] )); then
	ssh() { TERM=xterm-256color command ssh "$@"; }
fi

add-zsh-hook precmd _poopterm_precmd
add-zsh-hook preexec _poopterm_preexec
add-zle-hook-widget zle-line-init _poopterm_line_init
# <<< poopterm shell integration <<<
