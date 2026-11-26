# Charte graphique vibez

Référence de l'apparence de vibez : l'application, les instruments et les effets (DSP). Relevée dans le code le 26/11/2026 (version 0.6.8, branche `dev`, modifications locales comprises).

En cas d'écart entre ce document et le code, le code fait foi : corriger l'un des deux dans le même changement.

| Document | Contenu |
|---|---|
| README (ce fichier) | Marque, principes, couleurs, typographie, formes, icônes, rédaction, accessibilité, écarts connus |
| [Application](application.md) | Fenêtre, barre de contrôle, Browser, Arrange, Perform, Mix, piano roll, réglages, dialogues |
| [Instruments](instruments.md) | Cartes d'instruments, commun et détail, tableaux de paramètres |
| [DSP](dsp.md) | Cartes d'effets, grammaire des graphes, vumètres, tableaux de paramètres |
| [Thèmes](themes.md) | Les 20 thèmes intégrés, rôles de couleur, contrastes, format `.vzt` |

## 1. Identité

- Nom : **vibez**, toujours en minuscules, y compris en début de phrase dans l'interface.
- Nature : DAW libre pour la musique électronique, organisée en **Perform**, **Arrange** et **Mix**.
- Mot-symbole : « vibez » dans la couleur d'accent, en 18 px, en haut à gauche de la barre de contrôle.
- Icône : sillons concentriques (disque), un sillon accentué. Deux dessins : `assets/icon/vibez.svg` (sept sillons) et `assets/icon/vibez-small.svg` (quatre sillons épais, petites tailles).

| Couleur de l'icône | Valeur | Rôle |
|---|---|---|
| Fond | `#17161a` | carré arrondi, rayon 112 sur 480 |
| Sillons | `#cfc9bd` | opacité 0,42 (grand) ou 0,5 (petit) |
| Sillon accentué | `#c67c33` | un seul |

Ne pas recolorer, déformer ou redessiner l'icône. Son orange `#c67c33` est plus sourd que l'accent de Charcoal (`#fc871e`) : les deux sont des valeurs distinctes.

## 2. Principes

1. **Un instrument, pas un formulaire.** Fond sombre calme, couleur réservée à l'information : piste, état, signal.
2. **La couleur dit à quoi appartient une chose.** Clips, en-têtes, tranches et cartes d'appareils prennent la couleur de leur piste.
3. **Plat et rempli.** Les contrôles sont des aplats sans bordure ; l'état actif est un aplat de couleur avec encre sombre.
4. **Le graphe montre le DSP.** Une courbe est calculée par le code audio ou d'après lui, jamais décorative.
5. **Rien ne défile dans une carte.** Quand la place manque, la mise en page se replie.
6. **Conventions d'Ableton Live et de Bitwig** pour les gestes et le comportement ; l'apparence reste celle de vibez. Aucune ressource protégée copiée.

## 3. Couleur

Toute couleur vient de la palette courante (`crates/vibez-ui/src/theme.rs`, accesseurs `th::…`). Les thèmes se changent à l'exécution.

### Rôles (valeurs Charcoal, thème par défaut)

| Rôle | Charcoal | Porcelain | Usage |
|---|---|---|---|
| `display_bg` | `#101214` | `#e2e1de` | cadre de la fenêtre, puits, rainures, rail de fader |
| `bg_dark` | `#181a1c` | `#e9e8e5` | pistes de la timeline, fond des graphes, encre sur un aplat allumé |
| `bg_surface` | `#202225` | `#f5f3f0` | cartes de la fenêtre (panneaux) |
| `bg_elevated` | `#282a2d` | `#fdfcf9` | cartes d'appareils, tranches, menus, infobulles |
| `control` | `#323437` | `#e4e3e0` | boutons et listes au repos |
| `bg_hover` | `#3b3d40` | `#dcdad8` | survol |
| `text` | `#eaebee` | `#23211d` | texte principal, valeurs |
| `text_dim` | `#afb1b4` | `#575550` | noms de paramètres, légendes de 8 à 10 px |
| `text_muted` | `#84868a` | `#77746f` | indications, états désactivés |
| `accent` | `#fc871e` | `#e16c10` | marque, sélection, lecture d'état global |
| `accent_dim` | `#975112` | `#ea9858` | choix sélectionné dans un dialogue, focus de saisie |
| `border` | `#2d2f32` | `#dad9d6` | grilles des graphes (à 35 %), menus |
| `border_light` | `#3f4144` | `#c4c2c0` | bord d'infobulle et de dialogue |
| `success` | `#5edb81` | `#33ac5a` | confirmation, état sain |
| `danger` | `#f75d59` | `#d33a3c` | erreur, saisie invalide, Mute actif, enregistrement |
| `playhead` | blanc 80 % | noir 70 % | tête de lecture |

Rôles dérivés (ne pas les écrire en dur) : `field()`, `selected_place()`, `card_inset()`, `hairline(fond)` (6,5 % vers le texte), `groove()`, `track_header_hovered()`, `ruler_tick()`, `perform_*()`, `drop_zone_wash()`.

Mute actif = `danger`. Solo actif = `meter_yellow`.

### Échelle des surfaces

Les surfaces montent en clarté OKLCH en gardant la teinte du thème. Thème sombre, à partir de la base `b` : puits `b−0,035`, pistes `b`, panneaux `b+0,035`, cartes `b+0,07`, contrôles `b+0,11`, survol `b+0,145`.

### Couleurs de piste

Huit teintes en ordre arc-en-ciel, attribuées à la création (l'index boucle).

| # | Charcoal | Porcelain |
|---|---|---|
| 0 rouge | `#ed756e` | `#dd574e` |
| 1 orange | `#ec9247` | `#dd7b2b` |
| 2 jaune | `#dfc351` | `#d3a82d` |
| 3 vert | `#5ec077` | `#40a35c` |
| 4 cyan | `#39bcc3` | `#289da9` |
| 5 bleu | `#619eea` | `#4a81dc` |
| 6 violet | `#a37fde` | `#926bcf` |
| 7 magenta | `#e274ab` | `#cd5394` |

- Les clips de motif disposent de 16 couleurs : les 8, puis les 8 mélangées à 38 % de blanc.
- Préférence **Colour strength** : Vivid (défaut, couleurs du thème), Medium (saturation × 0,8), Soft (× 0,55). La teinte et la clarté ne changent pas.
- Les bandes d'EQ (`eq_lf`, `eq_lmf`, `eq_hmf`, `eq_hf`) reprennent l'orange, le bleu, le vert et le rouge du thème.

### Mélanger plutôt que rendre transparent

Pour un voile ou un fondu léger, utiliser `th::blend(fond, encre, quantité)`. Le moteur de rendu mélange l'alpha en lumière linéaire : une transparence faible paraît plusieurs fois plus forte que prévu, surtout en thème clair.

## 4. Typographie

| Rôle (`typography.rs`) | Police | Graisse | Usage |
|---|---|---|---|
| police par défaut d'iced | système | normale | noms de paramètres, texte courant |
| `PERFORM_DISPLAY` | IBM Plex Sans Condensed | 500 | grands titres de Perform |
| `PERFORM_LABEL`, `CLIP_TITLE`, `TRACK_TITLE` | IBM Plex Sans Condensed | 600 | étiquettes, noms de pistes et de clips |
| `PERFORM_TECH` | IBM Plex Mono | 500 | **toute valeur chiffrée**, raccourcis |
| `PERFORM_TECH_STRONG` | IBM Plex Mono | 600 | valeurs mises en avant (position, tempo) |
| `ICON_FONT` | Lucide | — | icônes |

Polices embarquées dans `assets/fonts/` (IBM Plex sous licence OFL). Elles sont enregistrées en largeur normale pour que les écritures absentes de Plex retombent sur une police système. Un nom saisi par l'utilisateur utilise `USER_TEXT_SHAPING`.

### Tailles (pixels logiques)

| Taille | Usage |
|---|---|
| 8 | étiquette de section en majuscules, graduations de vumètre, légende d'afficheur |
| 9 | texte des graphes, listes compactes, raccourcis |
| 10 | noms de paramètres, titres de groupe, infobulles (**base des appareils**) |
| 11 | valeurs, onglets, titres de clip, barre d'état |
| 11,5–12,5 | réglages |
| 12 | nom d'appareil (semi-gras), choix de la barre de contrôle |
| 13 | titres de dialogue et de panneau |
| 14–23 | titres et afficheur de position |

Ne pas descendre sous 8 px. Le code contient des 7 et 7,5 px : à ne pas étendre.

Texte trop long : couper avec « … » en mesurant dans la police réelle (`fit_text`, `fit_text_middle`). Jamais de retour à la ligne dans un libellé de contrôle.

## 5. Formes et espacement

### Rayons

| Rayon | Usage |
|---|---|
| 2–3 | champs de saisie, menus, infobulles, fond des graphes, boutons segmentés (extrémités à 3) |
| 4 | contrôles (`control_style::RADIUS`), pastille de numéro, dialogue de choix |
| 6 | petits boutons d'en-tête, segments d'un groupe, tuiles de clip, zones de dépôt |
| 7–8 | cartes d'appareils (8), tranches (8), groupes de segments (8), lignes du Browser (7) |
| 10 | cartes de la fenêtre (`shell_card`), groupes de réglages |
| 12 | feuille des réglages |

Plus l'élément est grand et extérieur, plus le rayon est grand.

### Espacement

Grille de 2 px. Pas courants : 2, 4, 6, 8, 10, 12, 16. Écart entre cartes de la fenêtre : 8 (`PANE_GAP`). Marge intérieure d'un appareil : 10 (`PAD`).

### Profondeur

La profondeur vient des surfaces, pas des ombres. Séparateurs : filets de 1 px (`divider`, `hairline`). Ombres réservées aux capuchons de fader, aux pads et aux fenêtres modales.

## 6. Icônes

Police Lucide uniquement (`icons.rs`), via `icons::icon(code)`. Tailles : 10 dans un titre de clip, 12 dans un en-tête d'appareil ou une arborescence, 13 dans le Browser et les zones de dépôt. Une icône seule a toujours une infobulle (`hint::with_hint`). Ajouter une icône = ajouter sa constante, pas un caractère en dur.

## 7. Contrôles et états

| État | Fond | Encre |
|---|---|---|
| Repos | `control` | `text_dim` |
| Survol | `bg_hover` | `text` |
| Allumé | couleur (piste ou accent) | `bg_dark` |
| Désactivé | `control` | `text_muted` |
| Segment choisi dans un groupe | `control` | `text` |
| Appareil contourné (bypass) | — | l'accent devient `text_muted` |

Aucune bordure sur un bouton. La bordure signale la saisie (1 px accent, `danger` si invalide) ou la sélection.

### Gestes d'une valeur

| Geste | Effet |
|---|---|
| Glisser vertical | course complète en environ 150 px |
| Maj + glisser ou molette | 5 fois plus fin |
| Molette | pas de 2 % |
| Double-clic (300 ms) | valeur par défaut |
| Clic sur la valeur | saisie au clavier, validée par Entrée ou perte du focus |

Un geste = une étape d'annulation. Une valeur pilotée par une automation affiche la valeur de la courbe à la tête de lecture.

### Raccourcis

Toujours des `ShortcutAction` (`shortcuts.rs`), donc modifiables. « Ctrl » s'écrit « Cmd » sur macOS. Affichage discret en Plex Mono 9 px à 60 % à côté du libellé.

## 8. Rédaction de l'interface

- Interface en anglais, vocabulaire des studios : Threshold, Ratio, Mix, Send, Return.
- Casse de phrase pour les boutons et menus (« Move earlier ») ; majuscules réservées aux petites étiquettes (`EQ`, `RETURN`, `PAN C`).
- Libellés courts : « Cutoff » plutôt que « Filter Cutoff » quand le groupe le dit déjà (`Param::label`).
- Unités après une espace : `-18.0 dB`, `5.0 ms`, `1.0 kHz`, `50 %`, `4.0 :1`, `0.0 st`, `-inf dB`, `L100 / C / R100`, `C4`.
- Hertz : `kHz` à partir de 1000. Secondes : `ms` sous 1 s.
- Chiffres avec point décimal.

## 9. Accessibilité

Seuils vérifiés par les tests des thèmes (`themes.rs`) :

| Élément | Contraste minimal |
|---|---|
| `text` sur toute surface | 7:1 |
| `text_dim` | 4,5:1 |
| `text_muted` | 3:1 |
| Poignée de fader | 3:1 |
| Anneau de potard | 1,4:1 |
| Lignes de grille (libellé / mesure / fine) | 1,5 / 1,2 / 1,12 |

- Interface Scale : de 75 % à 150 %, 100 % par défaut.
- Ne pas coder une information par la teinte seule : ajouter position, texte ou forme (le point d'alimentation s'entoure d'un halo quand il est allumé).
- Cible minimale constatée : 14 px (alimentation), 18 px (petit potard), 24 × 22 px (bouton d'en-tête). Ne pas faire plus petit.

## 10. Écarts connus

Relevés dans le code ; à corriger ou à accepter, pas à imiter.

| # | Écart | Où |
|---|---|---|
| 1 | Texte allumé (`bg_dark` sur accent) à 2,70:1 en Porcelain et 2,99:1 en Solarized Light, 3,57:1 en Berlin | `control_style.rs` |
| 2 | Couleurs des LED du mixeur écrites en dur, hors `meter_*` du thème | `widgets/console.rs` |
| 3 | Trois échelles de vumètre aux seuils différents | voir [DSP](dsp.md#vumètres) |
| 4 | Palettes fixes pour les tags (7) et la carte des sons (17), identiques dans tous les thèmes | `state/browser_tags.rs`, `state/sample_map.rs` |
| 5 | Voile des fenêtres modales : 50, 55, 62 ou 65 % selon la fenêtre | `app/views_*.rs` |
| 6 | Deux colonnes de potard : `param_column` (36 px, texte 9) et `Param::column` (texte 10 et 11) | `effect_knob.rs`, `device_controls.rs` |
| 7 | Potard de pan (arc 3 px) différent du potard d'appareil (arc 2,5 px) | `knob.rs` |
| 8 | « Voices » du Plucked String affiché `8.00` au lieu de `8` | `domains/param_entry.rs` |
| 9 | Le site (`site/index.html`) a sa propre palette et ses polices système | `site/` |

## 11. Liste de contrôle avant de livrer une interface

- [ ] Aucune couleur écrite en dur ; vérifié en Charcoal, Porcelain et High Contrast.
- [ ] Valeurs en Plex Mono, noms en `text_dim` 10 px.
- [ ] Accent = couleur de la piste, `text_muted` en bypass.
- [ ] Composants partagés réutilisés (`device_chrome`, `device_controls`, `device_layout`, `control_style`).
- [ ] Aucune barre de défilement à la hauteur minimale du panneau (180 px).
- [ ] Largeurs compacte, moyenne, étendue et maximale définies (`CardWidths`).
- [ ] Infobulle sur chaque icône seule ; raccourci paramétrable.
