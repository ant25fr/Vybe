# Priorités (comparaison avec Ableton Live et Bitwig)

À réaliser dans l'ordre. Les efforts sont des estimations.

| # | Priorité | Amélioration | Effort | État |
|---|---|---|---|---|
| 1 | 1 | Métronome activable, tap tempo, volume et raccourcis | Faible | Fait |
| 2 | 1 | MIDI mapping : MIDI learn des CC | Moyen | Fait (non testé avec un contrôleur) |
| 3 | 1 | MIDI : pitch bend et plusieurs ports d'entrée | Moyen | Fait pour les instruments intégrés, sauf Drum Rack (pas les plugins) |
| 4 | 1 | Sends pré/post-fader | Faible | Fait |
| 5 | 1 | Send d'un retour vers un autre retour (protection contre les boucles) | Moyen | Fait |
| 6 | 1 | Export des stems | Faible à moyen | Fait |
| 7 | 1 | Export MIDI | Faible à moyen | Fait (notes de l'Arrangement, sans swing) |
| 8 | 2 | Pistes de groupe | Moyen à élevé | Fait : imbrication, repli, ajout et retrait de pistes, y compris par glisser-déposer |
| 9 | 2 | Freeze / bounce en place | Moyen | Fait : freeze et bounce en place |
| 10 | 2 | Gamme et accords dans le piano roll | Faible à moyen | Fait : gamme et accords au dessin |
| 11 | 2 | Effets MIDI : arpégiateur, accords, gamme | Moyen | Fait pour les instruments intégrés (pas les plugins) |
| 12 | 2 | Prises multiples et comping | Élevé | Fait pour l’audio : prises en boucle, voies de prises, montage par glisser, écoute d’une prise (pas de MIDI) |
| 13 | 2 | Automation du tempo et signatures rythmiques | Élevé | Fait pour Arrange, avec limites (voir BUGS.md) : l’audio change de hauteur avec le tempo, Perform reste en 4/4 au tempo du projet |
| 14 | 3 | Modulateurs (LFO, enveloppe, aléatoire) | Élevé | Fait pour le volume, le panoramique et les appareils intégrés (pas les plugins ni les sends) |
| 15 | 3 | Racks avec macros et chaînes parallèles | Élevé | À faire |
| 16 | 3 | Follow actions des clips | Moyen | À faire |
| 17 | 3 | Ableton Link (licence à vérifier) | Moyen à élevé | À faire |
| 18 | 3 | MPE et expression par note | Moyen à élevé | À faire |
| 19 | Ajout | Séparateur de stems (voix, batterie, basse, autres) | Élevé | À faire |
| 20 | Ajout | Prise en charge d'ARA (ARA 2, type Melodyne) pour les plugins | Élevé | À faire |

## Notes

- **19, séparateur de stems** : demande un modèle d'apprentissage (par exemple
  Demucs ou un modèle ONNX). Le choix du modèle, sa licence, sa taille et son
  téléchargement sont à valider avant de commencer.
- **20, ARA** : non pris en charge aujourd'hui (aucune trace dans le code,
  vérifié le 26/11/2026). Demande le SDK ARA, son intégration à l'hébergement
  VST3 et CLAP, y compris dans le pont isolé, et l'accès aux clips audio. La
  licence du SDK est à vérifier.
