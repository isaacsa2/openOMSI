# High-resolution mirror redraw budget / Orçamento de atualização dos espelhos

## English

Experimental mitigation for [openOMSI#1549](https://github.com/openOMSI-org/openOMSI/issues/1549) and the new Discord report: a 512-pixel mirror setup caused large camera-dependent FPS drops, while lowering to 256 fixed the player's problem. Earlier reports also describe 1024-pixel mirrors causing much larger drops.

**Change:** desktop mirrors above 256px have a finite *total* redraw budget, without silently changing texture resolution: 512px at most 24 redraws per second overall; 1024px or larger at most 8 redraws per second overall; at most one high-resolution mirror redraw in one frame. 256px and smaller mirrors retain previous scheduling. The user's explicit "Every frame" setting, VR mirror path and mirror HUD panel update semantics stay unchanged. The rate may be below the former per-mirror minimum on buses with many mirrors. This trades reflection smoothness for predictable frame time and should be tested visually.

**A/B:** leave `OMSI_MIRROR_LEGACY_BUDGET` unset for the new behavior; set `OMSI_MIRROR_LEGACY_BUDGET=1` to restore the previous scheduling. This switch is a test control, not a recommended configuration.

**Test:** same machine/map/bus/view/time/preset/API (Vulkan and DX12), vehicle stationary, repeat at 256/512/1024 and Full/Eco refresh. Include camera movement beyond/within 12 meters from the bus, cabin view and upward view. Record frame-time p50/p95/p99, FPS, CPU mirror stage and GPU timestamps if supported (`OMSI_PROFILE=1`, `OMSI_GPU_TIMERS=1`). Compare screenshots and mirror motion; test at 20–30 and 60+ FPS with multiple mirror cameras. Do not use `OMSI_PROFILE_GPU=1` for normal throughput benchmarks.

No measured FPS improvement is claimed. Keep Draft until Windows release builds, unit tests, and on-device validation finish. Revert if scene stability or mirror visibility regresses.

## Português (Brasil)

Mitigação experimental para o [issue #1549](https://github.com/openOMSI-org/openOMSI/issues/1549) e o novo relato no Discord: espelhos configurados em 512 pixels causavam quedas fortes de FPS dependendo da câmera, e a redução para 256 resolveu o problema do jogador. Relatos anteriores descrevem quedas ainda maiores com espelhos de 1024 pixels.

**Alteração:** os espelhos do jogo em PC acima de 256 px recebem um limite total de atualização, sem reduzir silenciosamente a resolução escolhida: em 512 px, no máximo 24 atualizações por segundo no conjunto; em 1024 px ou mais, no máximo 8; e somente uma atualização de espelho de alta resolução por quadro. Até 256 px, o agendamento original permanece. A opção explícita "Every frame", os espelhos de VR e os painéis HUD continuam com seus controles próprios. Em ônibus com vários espelhos, a frequência individual poderá ficar abaixo do mínimo anterior. Isso troca parte da fluidez dos reflexos por menor custo por quadro, exigindo validação visual.

**A/B:** sem `OMSI_MIRROR_LEGACY_BUDGET`, o novo comportamento fica ativo. Com `OMSI_MIRROR_LEGACY_BUDGET=1`, retorna o agendamento antigo, apenas para comparação.

**Testes:** repetir mapa, ônibus, câmera, horário, preset e API (Vulkan/DX12). Testar 256/512/1024, Full/Eco, dentro e fora da cabine, aproximação dos 12 metros e câmera voltada para cima. Registrar FPS, p50/p95/p99, estágio de espelhos na CPU e tempos de GPU quando disponíveis. Conferir imagens e movimento dos reflexos, em 20–30 e 60+ FPS. Não usar `OMSI_PROFILE_GPU=1` para benchmarks representativos.

Ainda não há ganho comprovado de FPS. Manter em Draft até compilação, testes unitários e validação em hardware real. Reverter se houver regressão visual ou de estabilidade.
