# Application

Structure et apparence des écrans. Dimensions en pixels logiques.

## Fenêtre

```
┌ Barre de contrôle (52) : vibez · File · Edit · Perform/Arrange/Mix · transport · afficheur · grille · sortie · CPU ┐
│ ┌ Browser ┐ 8 ┌ Espace de travail (carte) ───────────────────────────────┐ │
│ │         │   └──────────────────────────────────────────────────────────┘ │
│ │         │ 8 ┌ Détail : Clip / Devices (180 minimum) ────────────────────┐ │
│ └─────────┘   └──────────────────────────────────────────────────────────┘ │
└ Barre d'état (26) : message · thème · sauvegarde · version · projet ──────┘
```

- Le cadre est peint en `display_bg`. Chaque région est une carte `shell_card` : fond `bg_surface`, rayon 10, filet de 1 px.
- L'écart de 8 px entre deux cartes est aussi la poignée qui les redimensionne.

## Barre de contrôle

- Posée directement sur le cadre, hauteur 52.
- Commandes regroupées en **groupes de segments** : carte `bg_surface`, filet, rayon 8, retrait 2 ; segments rayon 6. Segment actif : fond `control`, encre colorée (lecture en `success`, boucle et aimant en `accent`).
- Afficheur de position (hauteur 38) : mesure.temps.double-croche en grand Plex Mono, temps, tempo, signature (`SIG`), swing ; sous automation, le tempo est en lecture seule et sa légende `BPM AUTO` en accent ; légendes de 8 px en majuscules ; séparateurs d'un filet. Les valeurs éditables ressemblent à du texte jusqu'au focus (bordure `accent_dim`).
- Les trois vues forment un `segment_group::switch` ; le choix actif passe en semi-gras condensé.
- Sous 1440 px de large, la barre passe sur deux rangées.

## Browser

- Titre, champ de recherche (rayon 8, fond `field()`), colonne **Places** puis résultats.
- Lignes de 26 px, rayon 7, icône 12 à 13 ; indentation 8 px par niveau, 5 niveaux au plus.
- Lieu sélectionné : pastille `selected_place()` (accent à 16 % sombre, 13 % clair).
- Résultat : nom, métadonnées sur une seconde ligne, statut à droite.
- Pré-écoute en bas : interrupteur, RAW / WARP, lecture, forme d'onde, gain.
- Tags : 7 couleurs nommées (Red, Orange, Yellow, Green, Blue, Purple, Grey), 64 tags, 4 niveaux de dossiers.
- Zone de dépôt : trait pointillé 1,5 (5 pleins, 4 vides), voile très léger, étiquette de 26 px disant ce qui va se passer.

## Arrange

| Élément | Valeur |
|---|---|
| En-tête de piste | 219 + barre de couleur 4 |
| Règle | 36, dont bande de boucle 13 |
| Règle de temps (bas) | 20 |
| Vue d'ensemble | 34 |

- Règle : chaque changement de signature est écrit au pied de sa mesure, 8 px Plex Mono à l'accent ; numéros et grille suivent les signatures.
- En-tête : nom (police `TRACK_TITLE`) et type (MIDI, AUDIO) ; M, S, armement ; fader horizontal à la couleur de la piste avec repère 0 dB ; vumètre. L'action d'ajout n'apparaît qu'au survol.
- Pistes : fond du thème, bandes de mesure très légères (1,5 % vers le texte), grille à trois rangs. La boucle teinte la zone à 5 % d'accent (7 % en clair).

### Clips

- Titre de 16 px, aplat de la couleur de piste ; corps de la même couleur mêlée à la piste (28 % sombre, 18 % clair) à 55 % d'opacité en style Medium.
- Notes, formes d'onde et nom dans l'**encre** sombre du thème (alpha 0,86).
- Rayon 3, écart de 1 px entre clips ; titre 11 px, marge 6 ; icône de boucle à partir de 60 px de large.
- Sélection : titre éclairci de 33 % vers le blanc, contour `text` à 90 % de 1 px.
- Piste muette : toutes les parties mêlées à 55 % vers le fond ; le contour de sélection reste entier.
- Poignées de redimensionnement : 8 px (au plus 25 % du clip) ; fondu : rayon de prise 7.
- Poignées visibles sur le clip sélectionné et sur le clip survolé : fondus (carrés arrondis d'accent de 7 aux deux extrémités, à partir de 28 px de large) et volume (pastille d'accent de 16 × 5 au centre, sous le titre, à partir de 48 px), rayon 2,5 et contour `bg_dark` de 1 px.
- Volume : glisser vers le haut ou le bas (0,25 dB par pixel, Maj 5 fois plus fin), double-clic = 0 dB ; la valeur s'affiche sous la poignée dès qu'elle n'est pas nulle et la forme d'onde suit le gain.
- Panneau Clip : inspecteur de 388 de large ; sous Loop Start et Loop End, une rangée FADE IN / FADE OUT (course cubique) puis une rangée GAIN / PITCH, avec saisie de la valeur. Sur ce panneau, la hauteur minimale est de 430 pour montrer tous les potards, sans dépasser la hauteur maximale que la fenêtre autorise ; l'inspecteur défile quand le panneau est plus bas.
- Au-delà de la fin du clip : voile `bg_dark` à 60 %.

### Automation

Courbe à la couleur de la piste, points circulaires évidés, point saisi en `accent`. Nom « Appareil: Paramètre » dans la colonne de gauche.

## Perform

- Modes en haut : Sections (F1), Track Mutes (F2), Instrument (F3). État de transport en petites capitales techniques ; groupe d'enregistrement à droite.
- Grille de 16 pads (4 × 4). Un pad : numéro en Plex Mono dans la couleur de piste correspondant à son rang, position `R1 · C1`, nom centré, état en bas (`PLAYING`, `AVAILABLE`, `EMPTY`), bord coloré. Relief par `perform_pad_highlight()` et `perform_pad_lowlight()`.
- Pad en lecture : bord `accent`, dégradé de progression.
- Pad vide : « + SECTION » en `text_dim`.
- Clips : une colonne par piste ; tuiles rayon 6, écart 6, boutons de 20 ; anneau `accent` sur la tuile sélectionnée.
- Minimums : surface des pads 320, construction de Section 460, gouttière de pistes 112.

## Mix

| Élément | Valeur |
|---|---|
| Tranche | 172 de large, rayon 8, fond `bg_elevated` |
| EQ de tranche | 96 × 240 ; potards gain 40, fréquence 28 ou 24, Q 22 |
| Capuchon de fader | hauteur 30, rainures à ±4,5 et ±8 |
| Vumètre | 10 de large |
| Rangée d'envoi | 22 |

- Colonne gauche : en-tête (pastille de numéro à la couleur de la piste : `01`, `R1`, `M`), EQ, envois, pan, SEL / S / M. Colonne droite : fader et LED sur toute la hauteur, valeur dB en dessous.
- Master : couleur `accent`, ni pan ni boutons, mais la même hauteur.
- Tranche sélectionnée : trait de 2 px en haut, carte éclaircie de 3,5 % vers le texte.
- EQ de console : capuchons clairs pour les bandes extrêmes, sombres pour les médiums ; arc à la couleur de la bande ; boutons `EQ IN` et `BELL`.

### Loi des faders (`fader.rs`)

| Position de la course | Niveau |
|---|---|
| 100 % | +6 dB |
| 75 % | 0 dB |
| 55 % | −10 dB |
| 35 % | −20 dB |
| 15 % | −40 dB |
| 3 % | −60 dB |
| 0 % | silence |

Graduations affichées : 6, 0, −5, −10, −20, −40, −60.

## Détail : Clip et Devices

- Onglets `Clip`, `Devices`, puis nom de la piste. Rappel « Right-click to add a device » à droite.
- Chaîne d'appareils horizontale de gauche à droite, 6 px entre cartes, 140 px libres à la fin pour le clic droit, barre de défilement de 4 px sous les cartes.
- Ordre : instrument, puis effets, puis la carte **Modulators**.

### Carte Modulators

En-tête commun, couleur de la piste. Large de 180 sans modulateur, 620 sinon. Première rangée : un onglet par modulateur, `On / Off`, corbeille, boutons d'ajout. En dessous, à gauche (340) les menus Shape et Rate, la courbe (LFO : un cycle et un point de rayon 4 qui la parcourt ; enveloppe : ADSR remplie à 16 % avec trois poignées de rayon 4,5, la pointe réglant l'attaque, le coude le déclin et le maintien, le départ du relâchement sa durée ; la courbe occupe toute la largeur ; masquée sous 16 de haut) et des potards de 38 en colonnes de 60. À droite les cibles en rangées de 42, sur une colonne jusqu'à trois puis sur deux, qui défilent si le panneau est bas : potard d'amount bipolaire de 38, nom de l'appareil (10 px, numéroté s'il est en double) au-dessus du paramètre (12 px) et de la valeur (10 px), puis une croix ; l'infobulle donne le nom complet.

## Piano roll

| Élément | Valeur |
|---|---|
| Clavier | 52 de large |
| Hauteur d'une note (rangée) | 16 |
| Règle | 30 |
| Touche noire | 60 % de la largeur |
| Prise de redimensionnement | 6 |

Rangées `piano_white_row` / `piano_black_row`, ligne d'octave `piano_octave_line`. L'opacité d'une note suit sa vélocité. Couloir de vélocité sous les notes : barres de 3 à 7 px avec tête de 3 px.

## Réglages

- Feuille modale 900 × 640, marge 24, rayon 12 ; navigation de 200 avec lignes de 30.
- Groupes rayon 10 ; contrôles rayon 7, texte 11,5 ; libellé 12,5 ; aide 11,5.
- Interrupteur 34 × 20, pastille 14. Curseur de 260 : rail 4 (`accent` à gauche, `control` à droite), poignée ronde de rayon 7, valeur en Plex Mono sur 52.
- Tuiles de choix : hauteur 74, rayon 8.

## Dialogues, menus, infobulles

| Élément | Fond | Bord | Rayon |
|---|---|---|---|
| Dialogue de choix | `bg_surface`, retrait 18 | `border_light` | 6 |
| Choix | `bg_elevated` / `bg_hover` / `accent_dim` | `border` ou `accent` | 4 |
| Menu déroulant | `bg_surface` ou `bg_elevated` | `border` / `border_light` | 3 |
| Infobulle (10 px, sous le contrôle, à 6) | `bg_elevated` | `border_light` | 3 |

Un clic hors du dialogue le ferme.

## Barres de défilement

Épaisseur 4, invisibles au repos, `text_muted` à 50 % au survol ou pendant le glisser (`quiet_scroll.rs`).
