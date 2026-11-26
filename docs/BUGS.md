# Suivi des bugs

## Retour au début de la chaîne d'appareils (26/11/2026)

| Effet | Où | Statut |
|---|---|---|
| Ouvrir un menu, un dialogue ou glisser un échantillon changeait la racine de la vue : la chaîne d'appareils (et les autres zones défilantes) revenait à gauche | `app/views_shell.rs` | Corrigé, non compilé ni testé (consigne de session) |

## Session fondus, volume de clip et courbe LFO (26/11/2026)

À la demande de l'utilisateur, la session ne s'est pas terminée par une
compilation : les tests n'ont pas été lancés et les derniers réglages de hauteur
des poignées n'ont pas été compilés.

| Effet | Où | Statut |
|---|---|---|
| Saisir un fondu en secondes dans le panneau Clip ne passe pas par la même édition que la poignée : un fondu enchaîné lié peut rester incohérent | `domains/arrangement/audio_clip_inspector.rs` | Probable |
| Un potard de fondu ramené à une valeur identique crée une étape d’annulation vide | `domains/arrangement/messages.rs`, `defers_project_edit` | Probable |
| Les poignées survolées peuvent capter des clics proches du haut du corps d'un clip, auparavant pris par le déplacement du curseur ou le redimensionnement | `widgets/timeline/fade_drag.rs` | À vérifier dans l'application |
| La courbe du LFO suit la lecture d'Arrange ; à l'arrêt ou dans Perform, sa phase peut différer de celle du moteur (la vitesse est la même) | `app/modulator_devices.rs` | Limite connue |
| La courbe du LFO se rafraîchit à 30 images par seconde et ne montre pas fidèlement les fréquences au-delà d'environ 10 Hz | `app/mod.rs` | Limite connue |

## Corrigés le 26/11/2026 (session tempo, signatures et modulateurs)

Trouvés dans le code ajouté par la session, avant livraison.

| Effet | Où |
|---|---|
| En rendu par tranches, l'horloge continue des appareils avançait deux fois | `engine_tempo.rs`, `engine_graph.rs` |
| Un mute quantifié pouvait tomber jusqu'à une tranche trop tard | `engine_tempo.rs`, `SlicePlan::cut_before` |
| Supprimer un effet modulé laissait le rendu découpé en tranches pour toujours | `mixer/modulation.rs` |
| Bouncer en place perdait la modulation du volume et du panoramique | `app/track_freeze.rs` |
| On pouvait automatiser le tempo avec une piste gelée ou pendant un enregistrement | `app/frozen_guard.rs`, `app/update.rs` |
| Les barres de mesure de la voie d'un groupe supposaient quatre temps | `widgets/group_lane.rs` |

## Limites connues du tempo, des signatures et des modulateurs

| Effet | Piste | Statut |
|---|---|---|
| Sous automation du tempo, l'audio suit en changeant de hauteur (comme Re-Pitch) : pas d'étirement en direct, et les clips non warpés suivent aussi | `mixer/channel_strip.rs`, `render_repitched` | Limite connue (écart avec Live) |
| L'automation du tempo interdit l'enregistrement audio, le freeze et le bounce d'une piste | `app/audio_recording.rs`, `app/track_freeze.rs` | Limite connue |
| Les durées affichées en secondes sur la règle du bas supposent le tempo du projet | `widgets/timeline/time_ruler.rs` | Confirmé à la lecture |
| La compensation de latence et les positions entendues sont approchées sous automation du tempo | `engine_graph.rs`, horloges de voie | Probable |
| La répétition de note jouée pendant une lecture d'Arrange à tempo automatisé peut sauter des coups | `mixer.rs` | Probable |
| L'aimantation « Bar » et les barres du piano roll restent tous les quatre temps | `state/ui_types.rs`, `widgets/piano_roll/draw.rs` | Limite connue |
| Les libellés de longueur et l'inspecteur de clips comptent en 4/4 | `vibez-core/src/time.rs` | Limite connue |
| Perform, le lanceur de clips et le décompte comptent en 4/4 au tempo du projet | `engine_metronome.rs` | Limite connue |
| L'export DAWproject n'écrit que la première signature et pas l'automation du tempo | `vibez-project/src/dawproject.rs` | Limite connue |
| Un champ de signature saisi mais non validé reste affiché | `app/views_transport.rs` | Probable |
| Les potards d'un paramètre modulé montrent sa valeur centrale, sans animation | `widgets/device_controls.rs` | Limite connue |
| Dans Perform, la modulation est mise à jour une fois par bloc audio | `engine_section_queue.rs`, `engine_clip_launcher.rs` | Limite connue |
| Les plugins, sends, mute et swing ne sont pas modulables ; 4 modulateurs et 8 cibles par canal | `vibez-core/src/modulation.rs` | Limite connue |
| La carte Modulators a une largeur fixe, sans les quatre largeurs de la charte | `app/modulator_devices.rs` | Écart à la charte |

## Corrigés le 26/11/2026 (session pistes gelées, renommage, démarrage)

| Effet | Où |
|---|---|
| Dépôt, collage, édition ou prise sur une piste gelée y modifiait des clips masqués | `app/frozen_guard.rs` (refus avec message ; Capture passe la piste) |
| Une piste gelée était muette dans Perform | `engine_graph.rs`, le gel ne vaut que pour Arrange |
| Une piste « MIDI From » n'entendait plus une piste gelée | `engine_graph.rs`, la source joue son instrument sans être entendue |
| Geler pendant la lecture pouvait laisser des notes tenues | `engine_drain_commands.rs` |
| Bouncer en place demandait deux annulations | `app/bounce.rs` |
| La ligne d'un groupe n'était pas renommable dans Arrange, et le groupe figurait aussi parmi les retours | `app/views_arrangement.rs` |
| Les anneaux du bandeau About étaient recouverts par le fond | `widgets/about_banner.rs` |
| Monter ou descendre une piste pouvait séparer un groupe | `state/project_tracks.rs` |
| Glisser des clips par-dessus un groupe replié visait des pistes masquées | `domains/arrangement/clip_moves.rs` |
| Le glissement d'un en-tête n'avait pas de repère visuel | `app/header_drag.rs`, `app/views_arrangement.rs` |

## Corrigés le 26/11/2026 (session freeze, gammes, effets MIDI, groupes)

| Effet | Où |
|---|---|
| Ctrl+B pour « bounce en place » prenait le raccourci que le piano roll réserve | `shortcuts.rs` (trouvé par un test, corrigé avant livraison) |
| Dégeler ou bouncer en place demandait de confirmer la suppression d'une piste | `app/track_freeze.rs` |
| L'export DAWproject perdait l'imbrication des groupes | `vibez-project/src/dawproject.rs` |

## Écarts avec Live corrigés ensuite (26/11/2026)

Glisser-déposer des en-têtes dans les groupes, repli dans Mix, attente de
latence des pistes monitorées dans un groupe partagé, freeze sur la piste
elle-même, arpégiateur calé sur la grille, gamme enregistrée par clip et
appliquée au déplacement des notes, tranche d'un groupe placée avant ses
pistes dans Mix.

## Restants

| Effet | Piste | Statut |
|---|---|---|
| Bouncer en place une piste supprime ses prises (annulable) | `app/track_freeze.rs`, `finish_track_render` | Confirmé à la lecture |
| Pendant un enregistrement en boucle, la forme d'onde en direct dépasse la fin de boucle | `domains/audio_recording.rs`, aperçu linéaire | Confirmé à la lecture |
| Pas de renommage, déplacement ou rognage dans une voie de prise | `widgets/timeline/take_lane.rs` | Limite connue (écart avec Live) |
| L'écoute d'une prise n'est signalée que dans Arrange : elle continue sans repère dans Mix | `app/views_takes.rs` | Confirmé à la lecture |
| La voie d'un groupe ne montre ni les prises en cours d'enregistrement ni la sélection de temps, et n'accepte pas de dépôt d'échantillon | `widgets/group_lane.rs` | Limite connue |
| Le README, le site et l'installateur Windows renvoient encore au dépôt d'origine | `README.md`, `site/index.html`, `packaging/windows/vibez.iss` | Confirmé à la lecture |
| Pas de prises MIDI : Arrange n'enregistre que l'audio | `app/audio_recording.rs` | Limite connue |
| Les parties montées se raccordent par deux fondus de 4 ms, pas par un fondu enchaîné | `domains/arrangement/takes.rs` | Limite connue |
| Une version antérieure de vibez ouvrant le projet jouerait les prises par-dessus le montage | champ `take_lane` du fichier de projet | Limite connue |
| Coller des clips de plusieurs pistes sous un groupe replié peut viser ses pistes masquées | `domains/arrangement/clipboard.rs` | Probable |
| Une piste gelée ne joue pas ses clips figés dans Perform : elle rejoue ses instruments | Limite connue (Live fige aussi la vue Session) | Limite connue |
| Le pitch bend n'agit ni sur le Drum Rack ni sur les plugins | `vibez-instruments`, `vibez-plugin-host` | Limite connue |

## Zones relues

Export DAWproject, effets intégrés, entrée MIDI, tags du navigateur, sends,
groupes, freeze, démarrage, transport du moteur, rendu hors ligne, métronome.

## Pas encore relues

Perform et lanceur de clips, enregistrement audio, pont de plugins isolés,
warping, autosauvegarde.
