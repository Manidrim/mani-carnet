# Carnet

Notes et tâches en Rust (egui). Tout est stocké en Markdown lisible dans `~/Documents/Carnet/`
(un autre dossier est possible avec la variable d'environnement `CARNET_DIR`).

## Installer avec cargo
Une seule fois, l'outil qui fabrique le `.app` (5 à 7 minutes de compilation) :

    cargo install cargo-bundle

Puis, à chaque installation ou mise à jour :

    cd ~/RustInterface/carnet
    cargo bundle --release
    ditto target/release/bundle/osx/Carnet.app ~/Applications/Carnet.app
    open ~/Applications/Carnet.app

Ensuite : ⌘Espace → Carnet. Le raccourci `./install.sh` (ou un double-clic sur `Installer Carnet.command`) enchaîne ces commandes.

Sans `.app` (pour développer) : `cargo run --release`, ou `cargo install --path .` puis `carnet` dans le terminal.

## Tâches (⌘1)
Tape une tâche, puis Entrée. Coche la case pour la terminer. Double-clic ou ✏ pour la modifier.

| Tu tapes            | Effet                                        |
|---------------------|----------------------------------------------|
| `#octopus`          | tag (clique dessus pour filtrer)             |
| `@demain`, `@auj`   | échéance                                     |
| `@lundi` … `@dimanche` | le prochain jour de ce nom                |
| `@14h30`            | aujourd'hui à 14h30 (combinable : `@demain @9h`) |
| `@2026-11-03`       | date précise                                 |

Tri automatique : en retard, puis par échéance ; les tâches sans date ensuite ; les terminées en bas.
Avec un tag sélectionné, les nouvelles tâches et notes le reçoivent automatiquement.

## Rappels
Notification macOS à l'heure de l'échéance, ou à 9h le jour J si aucune heure n'est donnée.
Au lancement, un résumé unique s'affiche pour tout ce qui est déjà en retard.
La première fois, macOS peut demander d'autoriser les notifications de « Éditeur de script » : accepte-les.

## Notes (⌘2)
⌘N crée une nouvelle note. La sauvegarde est automatique et la recherche porte sur tout le texte. Les `#tags` dans le texte servent de filtres.

## Réglages (⌘,) : tout se configure en un clic
- **iCloud Drive** : le bouton « Déplacer mes données dans iCloud Drive » copie tout dans iCloud. La synchro entre tes Mac devient automatique, sans rien à configurer.
- **Git (GitLab…)** : crée un projet vide et privé, colle son adresse SSH, puis clique sur « Activer la synchro ».
  Si le dépôt contient déjà des données d'un autre Mac, elles sont fusionnées avec les tiennes : les tâches sont réunies, et une note en conflit est gardée sous `-importee`. L'ancien dossier est conservé en sauvegarde.
  La synchro est ensuite automatique : au lancement, toutes les 5 minutes et à la fermeture. ⌘S la force.
- **Lancement à l'ouverture de session** : une case à cocher, qui crée un LaunchAgent. C'est utile pour que les rappels sonnent même si tu n'as pas ouvert Carnet.
- **Notification de test** : pour vérifier que macOS les autorise.

## Format des fichiers
- `taches.md` : `- [ ] Appeler Greg #octopus @2026-11-03T14:30`
- `notes/*.md` : une note par fichier, le titre est la première ligne.

## Code
- `model.rs` : une tâche, la syntaxe `#tag` / `@date` et les tests
- `store.rs` : lecture et écriture des fichiers (écriture atomique)
- `config.rs` : réglages, migration iCloud sans perte, lancement auto
- `notify.rs` : thread de rappels et notifications via `osascript`
- `sync.rs` : configuration et synchronisation git en arrière-plan
- `main.rs` : interface egui
