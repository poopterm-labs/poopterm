# poopterm: loads shell integration through ZDOTDIR without touching your
# files. Restores your ZDOTDIR, runs your .zshenv, then loads the hooks.
if (( ${+POOPTERM_ORIG_ZDOTDIR} )); then
	ZDOTDIR=$POOPTERM_ORIG_ZDOTDIR
	unset POOPTERM_ORIG_ZDOTDIR
else
	unset ZDOTDIR
fi
if [[ -f ${ZDOTDIR:-$HOME}/.zshenv ]]; then
	builtin source -- "${ZDOTDIR:-$HOME}/.zshenv"
fi
if [[ -o interactive ]]; then
	builtin source -- "${${(%):-%x}:A:h}/poopterm.zsh"
fi
