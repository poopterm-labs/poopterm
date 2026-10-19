# Loaded by bash --rcfile. Read the usual login startup files before hooks.
# --rcfile is ignored by bash -l, so this interactive shell loads them here.
unset POOPTERM_BASH_RCFILE
if [[ -r /etc/profile ]]; then
	source /etc/profile
fi
if [[ -r ~/.bash_profile ]]; then
	source ~/.bash_profile
elif [[ -r ~/.bash_login ]]; then
	source ~/.bash_login
elif [[ -r ~/.profile ]]; then
	source ~/.profile
elif [[ -r ~/.bashrc ]]; then
	source ~/.bashrc
fi
# The hook source is appended at installation time.
