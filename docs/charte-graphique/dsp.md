# DSP : effets et graphes

Vingt et un effets intégrés (`EffectType`), une anatomie commune et une grammaire de graphes.

## Familles

| Famille | Effets |
|---|---|
| Niveau et correction | Gain, Filter, EQ |
| Dynamique | Compressor, Limiter, Gate, Multiband Dynamics |
| Saturation et numérique | Drive, Saturator, Bitcrush |
| Temps et espace | Delay, Echo, Reverb, Stereo Widener |
| Modulation | Phaser, Auto-Pan, Chorus, Flanger |
| Hauteur et fréquence | Pitch Shifter, Ring Modulator, Frequency Shifter |

Les familles servent à ce document ; elles ne changent pas la couleur : un effet prend toujours la couleur de sa piste.

## Carte d'effet

```
┌ ● Nom  [ Preset ▾ ] ‹ › ⌂ 💾                          ⚙ ˄ ˅ ✕ ┐  34
├──────────────────────────────────────────────────────────────────┤
│ bg_dark : Input │ graphe principal │ graphe secondaire │ GR      │  graphes
├──────────────────────────────────────────────────────────────────┤
│        potard      potard      potard      potard                │  74
├──────────────────────────────────────────────────────────────────┤
│ Advanced : onglets + une rangée                                  │  optionnel
└──────────────────────────────────────────────────────────────────┘
```

- En-tête : point d'alimentation (disque de 8 dans 14, halo d'accent à 18 % quand allumé), nom, preset et quatre boutons de 24 × 22, puis Advanced, monter, descendre, supprimer. Infobulle du nom : la latence.
- Bypass : accent remplacé par `text_muted`, nom en `text_dim`.
- Sidechain : colonne à gauche du corps, avant l'appareil (le routage se lit de gauche à droite).
- Potards 40 en colonnes de 68. Ordre des potards : du plus utilisé au moins utilisé (ex. Threshold, Ratio, Attack, Release, Knee, Makeup, Mix). **Mix en dernier.**
- Étroite : un sélecteur segmenté remplace les graphes côte à côte, et la rangée se limite aux contrôles essentiels.
- Hauteur : les graphes (72 minimum) partent d'abord, puis les potards passent sur une ligne, enfin Advanced prend tout le corps (`device_layout::stack`).
- Les effets simples (Chorus, Flanger, Pitch Shifter, Ring Modulator, Frequency Shifter) partagent un corps : graphes au-dessus d'une rangée de potards.
- Effet sans carte dédiée : une rangée de `param_column`.

## Largeurs

| Carte | Compacte | Moyenne | Étendue | Maximale |
|---|---|---|---|---|
| Gain | 368 | 579 | 579 | 820 |
| Filter | 420 | 484 | 829 | 1000 |
| EQ | 368 | 473 | 844 | 1100 |
| Compressor | 420 | 606 | 789 | 1000 |
| Limiter | 420 | 640 | 900 | 1200 |
| Gate | 420 | 420 | 677 | 1000 |
| Multiband Dynamics | 420 | 640 | 960 | 1200 |
| Drive | 420 | 420 | 580 | 1000 |
| Saturator | 420 | 640 | 1000 | 1200 |
| Bitcrush | 420 | 420 | 546 | 1000 |
| Delay | 420 | 420 | 760 | 1000 |
| Echo | 420 | 640 | 900 | 1200 |
| Reverb | 420 | 420 | 680 | 1000 |
| Phaser | 420 | 420 | 680 | 1000 |
| Auto-Pan | 420 | 420 | 496 | 1000 |
| Stereo Widener | 240 | 240 | 300 | 460 |
| Chorus | 380 | 380 | 480 | 840 |
| Flanger | 380 | 380 | 480 | 840 |
| Pitch Shifter | 380 | 380 | 480 | 840 |
| Ring Modulator | 380 | 380 | 480 | 840 |
| Frequency Shifter | 380 | 380 | 480 | 840 |

## Contrôles

| Besoin | Composant |
|---|---|
| Valeur continue | potard (`Param::column`) |
| Oui / non | interrupteur (`Param::switch`) allumé à l'accent |
| 2 à 4 choix visibles | `segmented` |
| Plus de choix | `dropdown` |
| Sections d'une carte | `tabs` ou `segmented` |
| Réglages rares | bouton Advanced de l'en-tête ou `disclosure` |
| Cut / boost | potard bipolaire (arc depuis le centre) |

### Potard

Course de 270° (de 135° à 405°). Anneau `knob_track` de 2,5 ; arc de valeur à l'accent de 2,5 aux bouts ronds, éclairci de 20 % au survol ; corps `knob_body` (`knob_body_engaged` au survol) de rayon inférieur de 4,5 ; index de 2. Diamètre de 36 à 76.

## Grammaire des graphes

| Élément | Règle |
|---|---|
| Fond | `bg_dark`, rayon 3 |
| Grille | `border` à 35 %, 1 px (0,3 à 0,4 pour les graphes d'instruments) |
| Référence (identité, signal sec) | `text_muted`, 1 à 1,5 px |
| Courbe principale | accent, 1,5 à 2 px, jonctions rondes |
| Remplissage sous courbe | accent à environ 16 % |
| Poignée | disque d'accent, rayon 5 (6 au survol), prise à +4 |
| Texte | 9 px `text_dim` ; valeur saisie 10 px `text` |
| Marges | 24 à gauche et 14 en bas pour les graduations, 4 en haut, 6 à droite |
| Titre | 10 px `text_dim` au-dessus, avec l'unité : « Gain reduction (dB) » |

- Une courbe vient des fonctions du DSP (`static_transfer_db`, `response_magnitude`) ou rejoue sa loi.
- Axe des fréquences logarithmique : 20, 100, 1k, 10k, 20k. Les graduations qui se chevaucheraient sont omises ; sous 60 px de haut, aucune.
- Tout ce qui se règle sur le graphe se règle aussi par un potard.
- Un geste sur le graphe = une étape d'annulation.
- Les canvas qui mettent leur géométrie en cache incluent `th::epoch()` pour suivre un changement de thème.

## Graphes par effet

| Effet | Graphes | Axe vertical | Interaction |
|---|---|---|---|
| Gain | historique de niveau, trims L/R | dB | potards |
| EQ | réponse sur spectre en direct, 4 poignées | dB, repères ±6 et ±12 | glisser = fréquence et gain, molette = Q, double-clic = zéro |
| Filter | réponse, forme de la modulation | +12 à −48 dB | glisser la coupure |
| Compressor | transfert, réduction dans le temps (5 s), vumètre GR | transfert −48 à 0 ; historique 24 dB ; GR 12 dB | glisser le seuil |
| Limiter | entrée avec réduction suspendue et plafond, GR | dB | — |
| Gate | historique du détecteur (4 s), ouverture, atténuation | 0 à −60 dB | glisser le seuil |
| Multiband Dynamics | niveau et réduction des 3 bandes | 0 à −60 dB | glisser chaque seuil pointillé |
| Drive | transfert avec entrée mesurée, sinus avant et après | +12 à −48 dB | — |
| Saturator | transfert, spectre harmonique | dB | trois nœuds en mode Custom |
| Bitcrush | oscilloscope d'un sinus au niveau d'entrée | amplitude | — |
| Delay | répétitions à venir | 0 à −48 dB | — |
| Echo | répétitions étalées, wow et flutter | 0 à −48 dB | — |
| Reverb | réponse impulsionnelle, ligne −60 dB | 0 à −60 dB | — |
| Phaser | réponse aux deux extrêmes, LFO | +12 à −24 dB | — |
| Auto-Pan | gain de chaque côté sur deux cycles, position stéréo | +6 à −24 dB | — |
| Chorus, Flanger | délais sur deux cycles, filtre en peigne | ms ; +12 à −24 dB | — |
| Pitch Shifter | transposition | −12 à +12 st | — |
| Ring Modulator, Frequency Shifter | bandes latérales d'un sinus de 1 kHz | dB | — |
| Stereo Widener | éventail (secteur de demi-cercle) qui s'ouvre avec Width | — | — |

Les bandes du Multiband Dynamics et de l'EQ utilisent les couleurs `eq_*`, seule exception à l'accent unique.

### Spectre de l'EQ

FFT de 2048 points, fenêtre de Hann, 96 bandes logarithmiques de 20 Hz à 20 kHz, plancher −66 dB, attaque immédiate, retour de 0,66 dB par rafraîchissement. Choix Pre / Post et Freeze.

## Vumètres

| Vumètre | Échelle | Vert → jaune → rouge | Couleurs | Usage |
|---|---|---|---|---|
| `LevelMeter` (44 de large, barres 9) | dBFS : 0, −6, −12, −24, −36, −48 espacés également | −12 / −3 dB | thème | cartes d'appareils : RMS, trait de crête |
| `LedMeter` (23 LED) | −60 à +3 dB, resserrée près de 0 | −6 / 0 dB | fixes `#4cff6a`, `#ffd23d`, `#ff4a3d` ; éteintes à 14 % | Mix : crête |
| `HorizontalVuMeterWidget` | amplitude linéaire | 0,6 / 0,85 (≈ −4,4 / −1,4 dB) | piste, puis thème | en-têtes de piste, sortie |

Pour un nouvel appareil, utiliser `level_column` (`LevelMeter`). La réduction de gain s'affiche en dB à une décimale, à l'accent.

## Paramètres

Valeurs telles que l'interface les formate, effet créé à 48 kHz. La latence indiquée est celle de l'état par défaut.

### Gain

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Gain | -inf dB à 6.0 dB | 0.0 dB |
| L Trim | -24.0 dB à 24.0 dB | 0.0 dB |
| R Trim | -24.0 dB à 24.0 dB | 0.0 dB |
| L Polarity | interrupteur | désactivé |
| R Polarity | interrupteur | désactivé |

### Filter

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Cutoff | 20 Hz à 20.0 kHz | 1.0 kHz |
| Resonance | 0.10 Q à 10.00 Q | 0.71 Q |
| Type | LP / HP / BP / Notch | LP |
| Slope | 12 dB / 24 dB | 12 dB |
| Drive | 0.0 dB à 24.0 dB | 0.0 dB |
| Mix | 0 % à 100 % | 100 % |
| Mod Source | Off / LFO / Envelope | Off |
| Rate | 0.05 Hz à 20 Hz | 1.00 Hz |
| Depth | 0.00 oct à 6.00 oct | 0.00 oct |
| Sync | Free / 1 bar / 1/2 / 1/4 / 1/8 / 1/16 / 1/4 T / 1/8 D | Free |
| Stereo Phase | 0 deg à 180 deg | 0 deg |
| Attack | 1.0 ms à 500.0 ms | 10.0 ms |
| Release | 10.0 ms à 2000.0 ms | 150.0 ms |

### EQ

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| LF Gain | -15.0 dB à 15.0 dB | 0.0 dB |
| LF Freq | 30 Hz à 450 Hz | 80 Hz |
| LF Bell | interrupteur | désactivé |
| LMF Gain | -15.0 dB à 15.0 dB | 0.0 dB |
| LMF Freq | 200 Hz à 2.5 kHz | 700 Hz |
| LMF Q | 0.30 à 4.00 | 0.70 |
| HMF Gain | -15.0 dB à 15.0 dB | 0.0 dB |
| HMF Freq | 600 Hz à 7.0 kHz | 2.5 kHz |
| HMF Q | 0.30 à 4.00 | 0.70 |
| HF Gain | -15.0 dB à 15.0 dB | 0.0 dB |
| HF Freq | 1.5 kHz à 16.0 kHz | 8.0 kHz |
| HF Bell | interrupteur | désactivé |
| LF Enabled | interrupteur | activé |
| LMF Enabled | interrupteur | activé |
| HMF Enabled | interrupteur | activé |
| HF Enabled | interrupteur | activé |
| Audition | Off / LF / LMF / HMF / HF | Off |
| HP Enabled | interrupteur | désactivé |
| HP Frequency | 20 Hz à 2.0 kHz | 20 Hz |
| LP Enabled | interrupteur | désactivé |
| LP Frequency | 1.0 kHz à 20.0 kHz | 20.0 kHz |
| Output | -24.0 dB à 24.0 dB | 0.0 dB |

### Compressor

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Threshold | -60.0 dB à 0.0 dB | -18.0 dB |
| Ratio | 1.0 :1 à 20.0 :1 | 4.0 :1 |
| Attack | 0.1 ms à 100.0 ms | 5.0 ms |
| Release | 10.0 ms à 500.0 ms | 80.0 ms |
| Makeup | 0.0 dB à 24.0 dB | 3.0 dB |
| Mix | 0 % à 100 % | 100 % |
| Knee | 0.0 dB à 24.0 dB | 0.0 dB |
| Detector HP | interrupteur | désactivé |
| HP Frequency | 20 Hz à 2.0 kHz | 120 Hz |
| Detector LP | interrupteur | désactivé |
| LP Frequency | 1.0 kHz à 20.0 kHz | 12.0 kHz |
| Detector Mode | Peak / RMS | Peak |
| Auto Release | interrupteur | désactivé |
| Stereo Link | interrupteur | activé |
| Detector Listen | interrupteur | désactivé |
| Range Enabled | interrupteur | désactivé |
| Range | 0.0 dB à 120.0 dB | 60.0 dB |

### Limiter

Latence : 283 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Input | -12.0 dB à 24.0 dB | 0.0 dB |
| Ceiling | -24.0 dB à 0.0 dB | -1.0 dB |
| Release | Auto à 500.0 ms | Auto |
| Style | Transparent / Punchy / Loud | Transparent |
| Lookahead | 0.5 ms à 5.0 ms | 2.0 ms |
| Stereo Link | interrupteur | activé |
| Quality | interrupteur | activé |
| True Peak | interrupteur | activé |

### Gate

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Threshold | -80.0 dB à 0.0 dB | -40.0 dB |
| Attack | 0.1 ms à 50.0 ms | 1.0 ms |
| Release | 5.0 ms à 500.0 ms | 50.0 ms |
| Hold | 0.0 ms à 200.0 ms | 10.0 ms |
| Hysteresis | 0.0 dB à 24.0 dB | 0.0 dB |
| Range Enabled | interrupteur | désactivé |
| Range | 0.0 dB à 120.0 dB | 60.0 dB |
| Mode | Gate / Expand | Gate |
| Expansion Ratio | 1.0 :1 à 20.0 :1 | 2.0 :1 |
| Detector HP | interrupteur | désactivé |
| HP Frequency | 20 Hz à 2.0 kHz | 80 Hz |
| Detector LP | interrupteur | désactivé |
| LP Frequency | 1.0 kHz à 20.0 kHz | 12.0 kHz |
| Detector Listen | interrupteur | désactivé |
| Mix | 0 % à 100 % | 100 % |

### Drive

Latence : 19 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Drive | 0.0 dB à 29.5 dB | 13.2 dB |
| Tone | 0 % à 100 % | 50 % |
| Output | -inf dB à 6.0 dB | 0.0 dB |
| Mix | 0 % à 100 % | 100 % |
| Character | Classic / Soft / Warm / Hard | Classic |
| Oversampling | 1x / 2x / 4x | 2x |
| Pre Filter | interrupteur | désactivé |
| Pre Frequency | 20 Hz à 1.0 kHz | 80 Hz |
| Post Filter | interrupteur | désactivé |
| Post Frequency | 1.0 kHz à 20.0 kHz | 12.0 kHz |

### Saturator

Latence : 19 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Drive | 0.0 dB à 36.0 dB | 5.0 dB |
| Bias | -100 % à 100 % | 0 % |
| Output | -24.0 dB à 12.0 dB | -2.0 dB |
| Mix | 0 % à 100 % | 100 % |
| Auto Gain | interrupteur | désactivé |
| Shape | Soft / Tube / Fold / Custom | Soft |
| Curve Amount | 0 % à 100 % | 75 % |
| Asymmetry | -100 % à 100 % | 0 % |
| Pre Filter | Off / 20 Hz HPF / 40 Hz HPF / 80 Hz HPF / 160 Hz HPF | 20 Hz HPF |
| Post Filter | Off / 20 kHz LPF / 16 kHz LPF / 12 kHz LPF / 8 kHz LPF / 4 kHz LPF | 20 kHz LPF |
| DC Removal | interrupteur | activé |
| Quality | 1x / 2x / 4x | 4x |
| Custom Low | -36.0 dB à 0.0 dB | -17.0 dB |
| Custom Mid | -12.0 dB à 12.0 dB | -1.0 dB |
| Custom High | -6.0 dB à 24.0 dB | 6.0 dB |

### Bitcrush

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Bits | 2 bit à 16 bit | 10 bit |
| SR Reduce | 48.0 kHz à 1.5 kHz | 48.0 kHz |
| Mix | 0 % à 100 % | 100 % |
| Drive | 0.0 dB à 24.0 dB | 0.0 dB |
| Rounding | Round / Floor / Truncate | Round |
| Dither | interrupteur | désactivé |
| Jitter | 0 % à 100 % | 0 % |
| Post LP Enabled | interrupteur | désactivé |
| Post LP Frequency | 20 Hz à 20.0 kHz | 12.0 kHz |

### Delay

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Time | 1.0 ms à 2000.0 ms | 500.0 ms |
| Feedback | 0 % à 95 % | 50 % |
| Mix | 0 % à 100 % | 30 % |
| Sync | Free / Sync | Free |
| Left Division | 1/64 / 1/32 / 1/16 / 1/8 / 1/4 / 1/2 / 1/1 | 1/4 |
| Left Modifier | Straight / Dotted / Triplet | Straight |
| Routing | Stereo / Ping-pong | Stereo |
| L/R Link | interrupteur | activé |
| Right Free Time | 1.0 ms à 2000.0 ms | 500.0 ms |
| Right Division | 1/64 / 1/32 / 1/16 / 1/8 / 1/4 / 1/2 / 1/1 | 1/4 |
| Right Modifier | Straight / Dotted / Triplet | Straight |
| Feedback HP Enabled | interrupteur | désactivé |
| Feedback HP Frequency | 20 Hz à 1.0 kHz | 120 Hz |
| Feedback LP Enabled | interrupteur | désactivé |
| Feedback LP Frequency | 1.0 kHz à 20.0 kHz | 8.0 kHz |
| Duck | 0 % à 100 % | 0 % |
| Time Change | Fade / Glide | Fade |

### Echo

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Time | 1.0 ms à 2000.0 ms | 375.0 ms |
| Sync | Free / Sync | Sync |
| Grid | 1/64 T à 1/1 D | 1/4 D |
| Feedback | 0 % à 98 % | 48 % |
| Mix | 0 % à 100 % | 25 % |
| Mode | Stereo / Ping-pong | Stereo |
| Character | 0 % à 100 % | 35 % |
| Tone | 0 % à 100 % | 55 % |
| Tape Character | Clean / Tape / Diffuse | Tape |
| Wear | 0 % à 100 % | 15 % |
| Wow | 0 % à 100 % | 8 % |
| Flutter | 0 % à 100 % | 4 % |
| Saturation | 0 % à 100 % | 12 % |
| Diffusion | 0 % à 100 % | 18 % |
| Duck | 0 % à 100 % | 20 % |
| Low Cut | 20 Hz à 2.0 kHz | 180 Hz |
| High Cut | 1.0 kHz à 20.0 kHz | 6.2 kHz |

### Reverb

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Room Size | 0.6 s à 10.7 s | 1.2 s |
| Damping | 0 % à 100 % | 50 % |
| Mix | 0 % à 100 % | 30 % |
| Pre-delay | 0.0 ms à 250.0 ms | 0.0 ms |
| Pre-delay Sync | Free / 1/64 / 1/32 / 1/16 / 1/16 D / 1/8 / 1/8 D / 1/4 | Free |
| Width | 0 % à 100 % | 100 % |
| Diffusion | 0 % à 90 % | 50 % |
| Low Cut | 20 Hz à 1.0 kHz | 20 Hz |
| High Cut | 1.0 kHz à 20.0 kHz | 20.0 kHz |
| Freeze | interrupteur | désactivé |

### Phaser

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Rate | 0.05 Hz à 8.00 Hz | 0.50 Hz |
| Depth | 0 % à 100 % | 70 % |
| Feedback | 0 % à 95 % | 30 % |
| Mix | 0 % à 100 % | 50 % |
| Centre | 50 Hz à 4.0 kHz | 800 Hz |
| Stages | 2 / 4 / 6 / 8 / 12 | 4 |
| Stereo Phase | 0 deg à 180 deg | 0 deg |
| FB Polarity | Positive / Negative | Positive |
| LFO Shape | Sine / Triangle | Sine |
| Sync | Free / 2 bars / 1 bar / 1/2 / 1/4 / 1/8 / 1/16 | Free |

### Auto-Pan

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Rate | 0.05 Hz à 16 Hz | 2.00 Hz |
| Depth | 0 % à 100 % | 70 % |
| Shape | Sine / Square / Triangle / Saw Up / Saw Down | Sine |
| Mode | Pan / Tremolo | Pan |
| Phase | 0 deg à 360 deg | 180 deg |
| Offset | 0 % à 100 % | 0 % |
| Smoothing | 0 % à 100 % | 0 % |
| Sync | Free / 2 bars / 1 bar / 1/2 / 1/4 / 1/8 / 1/16 / 1/32 | Free |
| Retrigger | Free Run / Song | Free Run |

### Multiband Dynamics

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Xover L/M | 40 Hz à 2.0 kHz | 250 Hz |
| Xover M/H | 500 Hz à 12.0 kHz | 3.5 kHz |
| Attack | 0.1 ms à 100.0 ms | 10.0 ms |
| Release | 10.0 ms à 1000.0 ms | 200.0 ms |
| Output | -24.0 dB à 24.0 dB | 0.0 dB |
| Low Thresh | -60.0 dB à 0.0 dB | -24.0 dB |
| Low Ratio | 1.0 :1 à 20.0 :1 | 4.0 :1 |
| Low Makeup | 0.0 dB à 24.0 dB | 0.0 dB |
| Mid Thresh | -60.0 dB à 0.0 dB | -24.0 dB |
| Mid Ratio | 1.0 :1 à 20.0 :1 | 4.0 :1 |
| Mid Makeup | 0.0 dB à 24.0 dB | 0.0 dB |
| High Thresh | -60.0 dB à 0.0 dB | -18.0 dB |
| High Ratio | 1.0 :1 à 20.0 :1 | 6.0 :1 |
| High Makeup | 0.0 dB à 24.0 dB | 0.0 dB |

### Chorus

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Rate | 0.05 Hz à 8.00 Hz | 1.20 Hz |
| Depth | 0 % à 100 % | 50 % |
| Delay | 5.0 ms à 40.0 ms | 18.0 ms |
| Feedback | 0 % à 90 % | 20 % |
| Mix | 0 % à 100 % | 50 % |

### Flanger

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Rate | 0.05 Hz à 10 Hz | 0.60 Hz |
| Depth | 0 % à 100 % | 80 % |
| Delay | 0.5 ms à 10.0 ms | 4.0 ms |
| Feedback | -0.95 à 0.95 | 0.40 |
| Mix | 0 % à 100 % | 50 % |

### Stereo Widener

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Width | 0.00 à 2.00 | 1.00 |
| Bass Mono | 0.00 Hz à 400 Hz | 0.00 Hz |

### Pitch Shifter

Latence : 9600 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Semitones | -12.00 st à 12.00 st | 0.00 st |
| Fine | -100.00 ct à 100.00 ct | 0.00 ct |
| Grain | 20.0 ms à 200.0 ms | 80.0 ms |

### Ring Modulator

Latence : 0 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Frequency | 10 Hz à 8.0 kHz | 440 Hz |
| Mix | 0 % à 100 % | 100 % |

### Frequency Shifter

Latence : 31 échantillons.

| Paramètre | Plage ou choix | Défaut |
|---|---|---|
| Shift | -2000.00 Hz à 2.0 kHz | 0.00 Hz |
| Mix | 0 % à 100 % | 100 % |
