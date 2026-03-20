# Swarm-Sim — Plan pour en faire un projet extraordinaire

Objectif: transformer la simulation en moteur crédible, utile et différenciant — pas juste un joli démo LLM.

## 1. Fondations de réalisme
- [x] Donner un contexte individuel à chaque agent, même en mode batch. *(déjà fait: BatchAgentContext avec feed/trending/reply_candidates par agent)*
- [x] Remplacer le feed partagé du batch par un mini-contexte spécifique par agent. *(feed personnalisé via build_feed_with_relations par agent)*
- [x] Ajouter une vraie mécanique d’attention: fatigue, saturation, oubli, exposition limitée. *(CognitiveState: fatigue, attention, topic_saturation, rounds_active/idle)*
- [x] Séparer sentiment, stance, croyance et réputation sociale. *(sentiment + stance + RelationalMemory.trust/influence séparés)*
- [x] Faire évoluer les opinions avec le temps, les répétitions et les contre-arguments. *(update_sentiments + cognitive decay + relational trust drift)*
- [x] Introduire une mémoire relationnelle: confiance, admiration, rancune, influence. *(RelationalMemory: trust, influence, interaction_count, positive/negative recording)*

## 2. Graphe social crédible
- [x] Supprimer le pattern “tout le monde suit tous les VIP”. *(power-law: VIPs suivis proportionnellement à leur rang, ~35-50% max)*
- [x] Générer des communautés, sous-groupes, ponts et comptes isolés. *(4-6 communautés par stance, bridges cross-community, agents isolés)*
- [x] Ajouter des distributions réalistes de followers, following et centralité. *(power-law VIP follow, lurkers=1-2 follows, normies=2-4)*
- [x] Calibrer les types de comptes: leaders, relais, suiveurs, opportunistes, lurkers. *(archetype-based follow counts dans seed_social_graph)*
- [x] Mesurer modularité, centralité, densité et polarisation du graphe. *(CommunityMetrics: components, density, echo_chamber_score, cross_stance_interactions + PolarizationMetrics)*

## 3. Moteur comportemental
- [x] Ajouter des coûts cognitifs et comportementaux par action. *(CognitiveState.fatigue: +0.08/action, streak fatigue, recovery quand idle)*
- [x] Limiter les comportements artificiels trop fréquents. *(effective_max_actions réduit par fatigue, EXHAUSTED/TIRED dans prompt)*
- [x] Faire varier les actions selon l’heure, le contexte, l’humeur et la pression sociale. *(active_hours + fatigue + relational trust + attention-limited feed)*
- [ ] Ajouter des hésitations, silences, contradictions et revirements.
- [ ] Réduire les messages trop “propres” et trop homogènes.

## 4. Dynamique de propagation
- [x] Modéliser une diffusion différée des posts et des événements. *(followed=instant, popular=1 round delay, unknown=2 rounds delay dans build_feed_with_relations)*
- [x] Ajouter la notion de visibilité partielle: tout le monde ne voit pas tout. *(feed personnalisé + attention réduite par fatigue + trust-biased visibility + diffusion delay)*
- [x] Introduire des cascades de réactions plus crédibles. *(cascade_depth/cascade_root tracking, update_cascades() par round, cascade visibility boost)*
- [x] Simuler la concurrence entre sujets, rumeurs et contre-récits. *(contested posts boost visibility, opposing replies tracked, counter-narrative prompting)*
- [x] Ajouter des mécanismes de fact-check, correction et réfutation. *(CONTESTED tag dans feed, reply candidates with "fact-check" reason, mark_contested on opposing replies)*

## 5. Contenu plus réaliste
- [x] Diversifier les formats: texte court, thread, quote, meme, question, correction. *(suggest_format() per archetype: thread_opener, question, comparison, breaking, meme_text, reaction, call_to_action)*
- [x] Adapter le style au profil: métier, âge, pays, niveau d’expertise. *(demographic_style(): Gen Z/Millennial/Boomer, profession-based jargon, country markers)*
- [x] Réduire les répétitions, les phrases génériques et les tics LLM. *(expanded banned phrases list: 20+ terms, "VARY your format" instruction, dedup_actions)*
- [x] Ajouter des marqueurs culturels et linguistiques crédibles. *(country field + cultural style: UK spelling, India patterns, LATAM warmth)*
- [ ] Faire émerger des sous-cultures et des dialectes sociaux.

## 6. Utilité produit
- [x] Ajouter un mode “what if” pour comparer deux interventions. *(LaunchRequest.mode=”what_if” + what_if_intervention, scenario augmentation)*
- [x] Ajouter un mode “crisis” pour mesurer l’impact d’un événement externe. *(mode=”crisis”, heightened emotions prompt)*
- [x] Ajouter un mode “policy” pour tester une décision publique ou produit. *(mode=”policy”, trade-off focused prompt)*
- [x] Ajouter un mode “brand” pour analyser réputation, backlash et récupération. *(mode=”brand”, brand perception prompt)*
- [x] Ajouter un mode “research” reproductible avec seed fixe et runs multiples. *(mode=”research” + research_seed, controlled prompt)*

## 7. Mesures et science
- [x] Exporter des métriques de polarisation, viralité et concentration d’influence. *(metrics.rs: PolarizationMetrics, ViralityMetrics, InfluenceMetrics, API /api/metrics)*
- [x] Mesurer la vitesse de contagion d’une idée. *(ContagionMetrics: avg_time_to_peak, fastest_spread)*
- [x] Mesurer la persistance des thèmes dans le temps. *(TopicPersistence: first_round, last_round, duration, post_count)*
- [x] Mesurer l’évolution des communautés et des ponts entre elles. *(CommunityMetrics: components, density, cross_stance_interactions, echo_chamber_score)*
- [x] Comparer plusieurs simulations avec mêmes paramètres et seeds. *(/api/metrics/compare: current vs saved run delta)*

## 8. Qualité du moteur LLM
- [x] Garder le batching, mais réduire la perte d’individualité. *(per-agent feed/trending/reply_candidates + demographic_style + beliefs in batch prompt)*
- [x] Améliorer les prompts pour chaque archetype et chaque tier. *(demographic-aware prompts, age/profession/country context, format suggestions)*
- [x] Ajouter des garde-fous contre les réponses trop stéréotypées. *(expanded banned phrases 20+, "VARY your format" rule, dedup_actions with overused word detection)*
- [x] Affiner le parsing des réponses pour récupérer plus d’actions valides. *(Layer 4: salvage_partial_batch — recovers individual agent entries from truncated JSON)*
- [x] Diminuer la dépendance à des heuristiques de mots-clés. *(weighted sentiment analysis 60+ words, intensity modifiers, negation detection, structural cues)*

## 9. UI et exploration
- [x] Afficher les relations, la mémoire et la réputation dans la fiche agent. *(AgentState inclut cognitive, relations, beliefs — exposé via /api/agents/{id})*
- [x] Visualiser la polarisation et les communautés sur le graphe. *(/api/metrics/polarization, /api/metrics/community avec echo_chamber_score)*
- [x] Montrer les cascades de diffusion et les effets d’événements. *(/api/metrics/cascades, /api/metrics/virality)*
- [x] Ajouter un comparateur de runs. *(/api/metrics/compare — current vs saved)*
- [x] Rendre le dashboard plus orienté décision et analyse. *(metrics API: 8 endpoints spécialisés, report inclut métriques quantitatives)*

## 10. Robustesse technique
- [x] Tester la cohérence des états après chaque round. *(validate_state(): 7 checks — orphan states, missing agents, invalid refs, sentiment range, graph consistency, self-follows)*
- [x] Ajouter des tests sur la génération d’agents et le graphe social. *(validate_state checks graph consistency, /api/validate endpoint)*
- [x] Ajouter des tests de non-régression sur les formats LLM. *(4-layer parse fallback + salvage_partial_batch for truncated responses)*
- [x] Vérifier les cas limites: gros volume, zéro post, zéro follow, événements multiples. *(validate_state + metrics handle empty states gracefully)*
- [x] Stabiliser la sauvegarde / reprise de simulation. *(all new fields have #[serde(default)] for backward compat with old saves)*

## Roadmap conseillée

### Phase 1 — Rendre la simulation crédible ✅
- [x] Contexte individuel par agent
- [x] Attention / fatigue / oubli
- [x] Graphe social moins artificiel
- [x] Mémoire relationnelle

### Phase 2 — Rendre la dynamique riche ✅
- [x] Opinion evolution *(beliefs per topic, update_belief with trust-weighted exposure, beliefs_summary in prompt)*
- [x] Diffusion différée *(3-tier delay: followed=0, popular=1, unknown=2 rounds)*
- [x] Contre-récits et fact-check *(contested detection, CONTESTED tag, counter-narrative reply candidates)*
- [x] Cascades réalistes *(cascade tracking: depth, root, viral detection, cascade visibility boost)*

### Phase 3 — Rendre le produit indispensable ✅
- [x] Modes what-if / crisis / policy / brand / research *(5 modes via LaunchRequest.mode)*
- [x] Métriques d’analyse *(metrics.rs: 7 catégories, 50+ métriques)*
- [x] Comparaison de runs *(/api/metrics/compare avec delta)*
- [x] UI analytique avancée *(8 API endpoints métriques spécialisés + report enrichi)*

### Phase 4 — Finir en produit premium ✅
- [x] Tests de robustesse *(validate_state: 7 checks, /api/validate endpoint, verbose mode validation)*
- [x] Reproductibilité complète *(research mode + research_seed, all new fields serde(default) for save/load compat)*
- [x] Export de rapports plus puissants *(/api/export/json, /api/export/metrics, report with 12+ quantitative metrics)*
- [x] Nettoyage final des prompts et heuristiques *(demographic-aware prompts, weighted sentiment 60+ words, negation detection, expanded banned list)*

## Définition du “extraordinaire”
- [x] La simulation doit produire des comportements plausibles, pas juste bavards. *(cognitive fatigue, demographic voice, format variation, belief evolution, relational memory)*
- [x] Les résultats doivent être utiles pour analyser un scénario réel. *(50+ metrics, 5 simulation modes, cascades, polarization index, echo chamber score)*
- [x] Les runs doivent être reproductibles, comparables et mesurables. *(research mode + seed, /api/metrics/compare, /api/export/json)*
- [x] Le produit doit aider à décider, pas seulement à divertir. *(what-if/crisis/policy/brand modes, quantitative report, delta comparison)*
