# Comparaison avec Bitwig Studio 6

Relevé du 27/11/2026 : les fonctions de Bitwig Studio 6 viennent des notes de
version officielles et de la presse (sources en bas), celles de Vybe d'une
recherche dans le code. Une absence signalée ici veut dire qu'aucune trace n'a
été trouvée dans le code.

## Ce que Vybe a déjà

| Domaine | Bitwig | Vybe |
|---|---|---|
| Clip launcher | Clips, scènes, next actions | Clips, lancement de ligne, modes Play, Legato, At End, follow actions |
| Performance | Clip launcher | Sections, pads, Capture vers l'arrangement |
| Modulation | Plus de 40 modulateurs, assignables partout | LFO, enveloppe, aléatoire sur volume, pan et appareils intégrés |
| Racks | Conteneurs imbriqués (couches, instruments, effets) | Effect Rack : 4 chaînes parallèles, 8 macros, effets intégrés seulement |
| Édition audio | Fades, gain, glissement dans le clip | Fades, gain, warp, découpe aux transitoires |
| Comping | Prises audio et notes | Prises audio |
| Plugins | VST, CLAP, bac à sable | VST3, CLAP, bac à sable |
| MIDI | Effets de notes, expressions MPE | Arpégiateur, accords, gamme ; pas de MPE |

## Ce qui manque

Classé par rapport entre l'utilité attendue et l'effort. Les numéros renvoient à
`PRIORITES.md`.

| # | Fonction (nom chez Bitwig) | Ce que cela apporterait | Effort |
|---|---|---|---|
| 21 | Probabilité et répétitions par note (Operators) | Des motifs qui varient d'un passage à l'autre | Moyen |
| 22 | Repères d'arrangement (cue markers) | Nommer les parties d'un morceau et y sauter | Faible à moyen |
| 23 | Export en 24 bits et 32 bits flottant | L'export est aujourd'hui en 16 bits seulement | Faible |
| 24 | Tonalité du projet (Global Key) | Piano roll et effets de notes suivent la même gamme | Faible à moyen |
| 25 | Saisie pas à pas (Step Input) | Écrire des notes au clavier sans jouer en rythme | Faible à moyen |
| 26 | Sauvegardes versionnées (Backups) | Revenir à un état plus ancien du projet | Faible |
| 27 | Mesure LUFS sur le master | Régler le niveau d'un mixage avant export | Moyen |
| 28 | Clips liés (Clip Aliases) | Modifier un motif répété en une fois | Moyen à élevé |
| 29 | Clips d'automation (Automation Clips) | Déplacer et répéter une automation comme un clip | Élevé |
| 30 | Sampler multi-échantillons (zones de notes et de vélocité) | Instruments échantillonnés réalistes | Élevé |
| 31 | Environnement modulaire (The Grid) | Créer ses propres instruments et effets | Très élevé |

Déjà inscrits dans `PRIORITES.md` : Ableton Link (17), MPE et expression par
note (18).

Limites des fonctions existantes qui rapprocheraient aussi de Bitwig : racks
avec plugins, racks imbriqués et racks d'instruments ; modulation des plugins et
des sends ; comping des notes MIDI.

## Ordre conseillé

Commencer par 23, 22 et 21 : peu risqués, visibles tout de suite, et sans
changement profond du moteur audio. La 31 est un projet à part entière et n'est
pas conseillée pour l'instant.

## Sources

- [Notes de version Bitwig Studio 6.0](https://downloads.bitwig.com/6.0/Release-Notes-6.0.html)
- [MusicTech : Bitwig Studio 6](https://musictech.com/news/gear/bitwig-studio-6/)
- [Synth Anatomy : Bitwig Studio 6.1](https://synthanatomy.com/2026/08/bitwig-studio-6.html)
- [Guide utilisateur Bitwig : The Grid](https://bitwig.com/userguide/latest/the_grid_devices)
