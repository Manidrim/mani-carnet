#!/usr/bin/env bash
# Double-clic dans le Finder : compile et installe Carnet.app dans ~/Applications.
cd "$(dirname "$0")"
bash install.sh
status=$?
echo
if [ $status -eq 0 ]; then echo "Terminé. Tu peux fermer cette fenêtre."; else echo "Erreur ($status). Copie le texte ci-dessus à Claude."; fi
read -n 1 -s -r -p "Appuie sur une touche pour fermer…"
