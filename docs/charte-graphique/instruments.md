# Instruments

Six instruments intégrés : Subtractive Synth, Wavetable Synth, FM Synth, Sampler, Drum Rack, Plucked String. Les plugins VST3 et CLAP ont une carte générique.

## Anatomie commune

```
┌ En-tête 34 : ● Nom  [ Preset ▾ ] ‹ › ⌂ 💾                        ✕ ┐
├──────────────────────────────────────────────────────────────────────┤
│ Onglets 24 (soulignés à la couleur de la piste)                      │
├──────────────────────────────────────────────────────────────────────┤
│ Bande : [titre + menus]          │ [titre + menus]                   │
│         graphe  │ potards        │ graphe │ potards                  │
├──────────────────────────────────────────────────────────────────────┤
│ Bande partagée (vue d'ensemble ou voix, mix, volume)                 │
└──────────────────────────────────────────────────────────────────────┘
```

| Élément | Règle |
|---|---|
| Carte | fond `bg_elevated`, rayon 8 |
| En-tête | 34, retraits 12 à gauche et 8 à droite, nom 12 semi-gras (32 minimum), preset de 72 à 200 |
| Alimentation | un instrument est toujours allumé : point sans action |
| Accent | couleur de la piste, partout dans la carte |
| Bande de graphe | fond `bg_dark`, pleine largeur, retrait 6 vertical |
| Titre de groupe | 10 px, `text_dim` |
| Séparateurs | filets `divider` de 1 px, horizontaux et verticaux |

## Largeurs

| Carte | Compacte | Moyenne | Étendue | Maximale |
|---|---|---|---|---|
| Subtractive Synth | 440 | 700 | 1100 | 1300 |
| Wavetable, FM | 440 | 700 | 1100 | 1300 |
| Sampler | 440 | 680 | 680 | 1300 |
| Drum Rack | 440 | 440 | 681 | 1600 |
| Plucked String | 334 | 334 | 334 | 534 |

Compacte tant que la mise en page moyenne ne tient pas ; ensuite la carte prend la place disponible jusqu'au maximum. Une chaîne trop pleine défile au lieu d'écraser les cartes.

## Repli en hauteur

1. Graphes et potards.
2. Les graphes disparaissent d'abord.
3. Les potards passent sur une ligne (24) : nom, valeur, petit potard de 18.
4. Cartes à bandes : un menu des parties et les contrôles de la partie sur une seule ligne.

Une rangée de potards avec noms et valeurs fait 74.

## Subtractive Synth

- Onglets : Oscillator, Filter, Envelope, Modulation. À partir de 1100, plus d'onglets : tout est visible.
- Largeur moyenne (700) : la section choisie à côté de l'enveloppe de volume.
- Potards 30 ; menus de 92 ; titre de section 26 ; graphe de 40 au minimum.
- Graphes : forme d'onde, réponse du filtre, enveloppe, LFO.
- Quand la hauteur manque, la rangée partagée se replie avant les graphes.

## Wavetable Synth

- Onglets : Oscillators, Filter, Envelopes, Modulation. Vue d'ensemble à deux bandes en largeur complète sur Oscillators.
- Mise en page à bandes (`instrument_bands.rs`) : potards 38 en colonnes de 60, menus de 110.
- Graphe signature : toutes les images de la table empilées en fines ondes de deux cycles ; l'image jouée en gras, à sa place, déformée par Warp ; Position marquée d'un point.
- Tables : Basic Shapes, Harmonic, Formant, Pulse, Sync.

## FM Synth

- Onglets : Overview, Operators, Envelopes, Modulation. Quatre opérateurs A à D.
- Graphe d'algorithme : opérateurs reliés par des flèches ; un clic choisit l'opérateur affiché.
- Oscilloscope : deux cycles du son à sa brillance maximale.
- Huit algorithmes, affichés par leur routage (ex. `D → C → B → A`).

## Sampler

- Onglets : Sample, Zones, Pitch, Filter, Modulation.
- En tête de corps : nom du fichier, durée, Root, Tune, Gain, Pan, Load.
- Forme d'onde avec marqueurs Start, End et boucle, tous déplaçables ; axe en secondes.
- Colonne Mode : One-shot, Gate, Loop ; Reverse ; Xfade.
- Zones : carte tonalité × vélocité.

## Drum Rack

- 16 pads par banque, banques A à D ; menu Kit ; colonne de 160.
- Pad : biseau 2, face rayon 5, biseau rayon 7 ; nom défilant s'il est trop long (délai 0,4 s, 28 px/s) ; mini forme d'onde.
- Pad sélectionné : accent à 95 % ; non sélectionné : 60 %. Pad vide : voile de 5 % de texte.
- Chaque pad est un Sampler complet, sans l'onglet Zones.
- Choke : Off, 1, 2, 3, 4.
- Côte à côte (grille et Sampler) à partir de 681 ; la grille mesure de 240 à 340.

## Plucked String

- Pas d'onglets. En haut : Envelope et Loop Filter ; au milieu : **Strings**, cordes pinçables à la souris (position horizontale = une octave, verticale = la corde) ; en bas : deux rangées de petits potards (30) en colonnes de 58.
- Les graphes reçoivent 70 % de la hauteur en plus.
- Sous 2 × 76 + 56 de hauteur, les potards passent sur des lignes.

## Plugins

Carte de 196 : étiquette `VST3 PLUGIN` ou `CLAP PLUGIN` en majuscules 8 px, latence, bouton « Open editor » ou « No editor ». Le plugin dessine ses réglages dans sa propre fenêtre.

## Paramètres

Valeurs telles que l'interface les formate (hôte à 48 kHz). Le Drum Rack n'a pas de paramètre propre : ceux du Sampler s'appliquent à chaque pad.

### Subtractive Synth

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Waveform | Sine / Saw / Square / Triangle | Saw |
| Attack | 1 ms à 2.00 s | 10 ms |
| Decay | 1 ms à 2.00 s | 100 ms |
| Sustain | 0 % à 100 % | 70 % |
| Release | 1 ms à 5.00 s | 300 ms |
| Filter Cutoff | 20 Hz à 20.0 kHz | 5.0 kHz |
| Filter Res | 0.10 Q à 10.00 Q | 1.00 Q |
| Volume | -inf dB à 0.0 dB | -1.9 dB |
| Octave | -3 oct à 3 oct | 0 oct |
| Detune | -12.0 st à 12.0 st | 0.0 st |
| Osc 2 Wave | Sine / Saw / Square / Triangle | Triangle |
| Osc 2 Octave | -3 oct à 3 oct | 0 oct |
| Osc 2 Detune | -12.0 st à 12.0 st | 0.0 st |
| Osc 2 Level | -inf dB à 0.0 dB | -inf dB |
| Sub | -inf dB à 0.0 dB | -inf dB |
| Noise | -inf dB à 0.0 dB | -inf dB |
| Filter Type | LP12 / LP24 / HP12 / BP12 | LP12 |
| Voice Mode | Poly / Mono / Legato | Poly |
| Voices | 1 à 8 | 8 |
| Glide | 0.0 ms à 2000.0 ms | 0.0 ms |
| LFO Shape | Sine / Triangle / Square / Saw | Sine |
| LFO Target | Cutoff / Pitch / Volume | Cutoff |
| LFO Rate | 0.05 Hz à 20 Hz | 1.20 Hz |
| LFO Amount | 0 % à 100 % | 0 % |
| Filter Env | -4.0 oct à 4.0 oct | 0.0 oct |
| Filter Attack | 1 ms à 2.00 s | 5 ms |
| Filter Decay | 1 ms à 2.00 s | 300 ms |
| Filter Sustain | 0 % à 100 % | 0 % |
| Filter Release | 1 ms à 5.00 s | 300 ms |

### Wavetable Synth

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Osc A Table | Basic Shapes / Harmonic / Formant / Pulse / Sync | Harmonic |
| Osc A Position | 0 % à 100 % | 66 % |
| Osc A Warp | 0 % à 100 % | 0 % |
| Osc A Detune | 0 ct à 50 ct | 0 ct |
| Osc A Unison | 1 à 8 | 1 |
| Osc A Level | 0 % à 100 % | 80 % |
| Osc A Octave | -3 oct à 3 oct | 0 oct |
| Osc B Table | Basic Shapes / Harmonic / Formant / Pulse / Sync | Harmonic |
| Osc B Position | 0 % à 100 % | 30 % |
| Osc B Warp | 0 % à 100 % | 0 % |
| Osc B Detune | 0 ct à 50 ct | 0 ct |
| Osc B Unison | 1 à 8 | 1 |
| Osc B Level | 0 % à 100 % | 0 % |
| Osc B Octave | -3 oct à 3 oct | 0 oct |
| Filter Type | LP12 / LP24 / HP12 / BP12 | LP24 |
| Cutoff | 20 Hz à 20.0 kHz | 20.0 kHz |
| Resonance | 0 % à 100 % | 10 % |
| Drive | 0 % à 100 % | 0 % |
| Attack | 1 ms à 2.00 s | 10 ms |
| Decay | 1 ms à 2.00 s | 300 ms |
| Sustain | 0 % à 100 % | 80 % |
| Release | 1 ms à 5.00 s | 300 ms |
| LFO Shape | Sine / Triangle / Square / Saw | Sine |
| LFO Rate | 0.05 Hz à 20 Hz | 1.00 Hz |
| LFO Amount | 0 % à 100 % | 0 % |
| LFO Destination | Osc A Position / Osc B Position / Osc A Warp / Osc B Warp / Filter Cutoff / Pitch / Volume | Osc A Position |
| Env 2 Attack | 1 ms à 2.00 s | 5 ms |
| Env 2 Decay | 1 ms à 2.00 s | 400 ms |
| Env 2 Sustain | 0 % à 100 % | 0 % |
| Env 2 Release | 1 ms à 5.00 s | 300 ms |
| Env 2 Amount | -100 % à 100 % | 0 % |
| Env 2 Destination | Osc A Position / Osc B Position / Osc A Warp / Osc B Warp / Filter Cutoff / Pitch / Volume | Filter Cutoff |
| Voices | 1 à 8 | 8 |
| Output | -24.0 dB à 6.0 dB | -6.0 dB |

### FM Synth

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Op A Ratio | 0.50 à 16.00 | 1.00 |
| Op A Fine | -0.50 à 0.50 | 0.00 |
| Op A Level | -60.0 dB à 0.0 dB | 0.0 dB |
| Op A Feedback | 0 % à 100 % | 0 % |
| Op A Key Track | 0 % à 100 % | 0 % |
| Op A Attack | 1 ms à 2.00 s | 5 ms |
| Op A Decay | 1 ms à 5.00 s | 600 ms |
| Op A Sustain | 0 % à 100 % | 70 % |
| Op A Release | 1 ms à 5.00 s | 300 ms |
| Op B Ratio | 0.50 à 16.00 | 1.00 |
| Op B Fine | -0.50 à 0.50 | 0.00 |
| Op B Level | -60.0 dB à 0.0 dB | -18.0 dB |
| Op B Feedback | 0 % à 100 % | 0 % |
| Op B Key Track | 0 % à 100 % | 0 % |
| Op B Attack | 1 ms à 2.00 s | 5 ms |
| Op B Decay | 1 ms à 5.00 s | 600 ms |
| Op B Sustain | 0 % à 100 % | 70 % |
| Op B Release | 1 ms à 5.00 s | 300 ms |
| Op C Ratio | 0.50 à 16.00 | 1.00 |
| Op C Fine | -0.50 à 0.50 | 0.00 |
| Op C Level | -60.0 dB à 0.0 dB | -60.0 dB |
| Op C Feedback | 0 % à 100 % | 0 % |
| Op C Key Track | 0 % à 100 % | 0 % |
| Op C Attack | 1 ms à 2.00 s | 5 ms |
| Op C Decay | 1 ms à 5.00 s | 600 ms |
| Op C Sustain | 0 % à 100 % | 70 % |
| Op C Release | 1 ms à 5.00 s | 300 ms |
| Op D Ratio | 0.50 à 16.00 | 1.00 |
| Op D Fine | -0.50 à 0.50 | 0.00 |
| Op D Level | -60.0 dB à 0.0 dB | -60.0 dB |
| Op D Feedback | 0 % à 100 % | 0 % |
| Op D Key Track | 0 % à 100 % | 0 % |
| Op D Attack | 1 ms à 2.00 s | 5 ms |
| Op D Decay | 1 ms à 5.00 s | 600 ms |
| Op D Sustain | 0 % à 100 % | 70 % |
| Op D Release | 1 ms à 5.00 s | 300 ms |
| Algorithm | D → C → B → A ; C + D → B → A ; C → B → A ← D ; D → C → A ← B ; D → C, B → A ; D → A, B, C ; D → C, A, B ; A + B + C + D | D → C → B → A |
| Brightness | 0 % à 100 % | 50 % |
| Decay | 0 % à 100 % | 50 % |
| Motion | 0 % à 100 % | 0 % |
| LFO Shape | Sine / Triangle / Square / Saw | Sine |
| LFO Rate | 0.05 Hz à 20 Hz | 5.00 Hz |
| LFO Destination | Brightness / Pitch / Volume | Brightness |
| Velocity | 0 % à 100 % | 50 % |
| Voices | 1 à 8 | 8 |
| Volume | -24.0 dB à 6.0 dB | -6.0 dB |

### Sampler

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Root Note | C-1 à G9 | C4 |
| Attack | 1 ms à 2.00 s | 1 ms |
| Decay | 1 ms à 2.00 s | 100 ms |
| Sustain | 0 % à 100 % | 100 % |
| Release | 1 ms à 5.00 s | 10 ms |
| Start | 0 % à 100 % | 0 % |
| End | 0 % à 100 % | 100 % |
| Loop | interrupteur | désactivé |
| One-Shot | interrupteur | désactivé |
| Volume | -inf dB à 0.0 dB | -1.9 dB |
| Loop Start | 0 % à 100 % | 0 % |
| Loop End | 0 % à 100 % | 100 % |
| Reverse | interrupteur | désactivé |
| Loop Crossfade | 0.0 ms à 500.0 ms | 0.0 ms |
| Tune | -24.0 st à 24.0 st | 0.0 st |
| Fine | -100 ct à 100 ct | 0 ct |
| Glide | 0.0 ms à 2000.0 ms | 0.0 ms |
| Key Low | C-1 à G9 | C-1 |
| Key High | C-1 à G9 | G9 |
| Velocity Low | 1 à 127 | 1 |
| Velocity High | 1 à 127 | 127 |
| Velocity | 0 % à 100 % | 100 % |
| Filter Type | Off / LP12 / LP24 / HP12 / BP12 | Off |
| Filter Cutoff | 20 Hz à 20.0 kHz | 20.0 kHz |
| Filter Res | 0.10 Q à 10.00 Q | 0.71 Q |
| Filter Env | -4.0 oct à 4.0 oct | 0.0 oct |
| Filter Attack | 1 ms à 2.00 s | 5 ms |
| Filter Decay | 1 ms à 2.00 s | 300 ms |
| Filter Sustain | 0 % à 100 % | 0 % |
| Filter Release | 1 ms à 5.00 s | 300 ms |
| LFO Shape | Sine / Triangle / Square / Saw | Sine |
| LFO Target | Cutoff / Pitch / Volume | Cutoff |
| LFO Rate | 0.05 Hz à 20 Hz | 1.20 Hz |
| LFO Amount | 0 % à 100 % | 0 % |
| Pan | L100 à R100 | C |

### Plucked String

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Attack | 1 ms à 200 ms | 5 ms |
| Decay | 20 ms à 5.00 s | 400 ms |
| Sustain | 0 % à 100 % | 60 % |
| Release | 20 ms à 5.00 s | 250 ms |
| Damping | 0 % à 100 % | 60 % |
| Stretch | 0 % à 100 % | 0 % |
| Velocity | 0 % à 100 % | 80 % |
| Voices | 1.00 à 16.00 | 8.00 |
| Volume | -60.0 dB à 6.0 dB | -3.0 dB |
