# Older NVIDIA Windows driver safety profile / Perfil de segurança para NVIDIA antigas no Windows

## English

Investigates reports of whole-PC hangs with NVIDIA GeForce GT 730 and GTX 750/750 Ti, some recorded as `LiveKernelEvent 141` (`VIDEO_ENGINE_TIMEOUT_DETECTED`, including `nvlddmkm.sys`). **This is a mitigation, not a demonstrated fix for GPU timeouts**: a 141 event can involve drivers, hardware, thermals, power supply, or workloads, and cannot be diagnosed solely from the game.

**Change (Windows only):** when the detected NVIDIA vendor/name identifies a GT 730 or GTX 750 family, automatically use MSAA 1x, anisotropy at most 2x, no SSAO, shadow size at most 1024, no experimental ray queries, no optional pipeline cache feature, and conservative WebGPU buffer/device limits. BC/DXT compressed textures remain supported where the adapter supports them (to avoid increasing VRAM use). No global setting, protocol, Windows TDR timeout, or unrelated GPU is changed.

**Escape hatch:** `OMSI_NVIDIA_LEGACY_FULL_GPU=1` restores the requested full path for a controlled A/B if the driver is known stable. **Do not ask users with full-system hangs to enable this flag as a casual test.**

**Validation:**
- Run `cargo test -p omsi-render -p omsi-cfg` and a Windows Release build.
- Test startup, menu, Grundorf and heavy map without texture corruption or driver resets on GT 730 and GTX 750, then on a modern NVIDIA card as negative control.
- On unstable PCs, stop running the affected build after a driver hang; collect `game.log`, GPU/driver identifiers, Windows Reliability Monitor timestamps, and `C:\Windows\LiveKernelReports\WATCHDOG` dumps if available. Compare with a previously stable release before further experiments.
- Correlate frame diagnostics with TDR evidence only after collecting real hardware data. Never increase `TdrDelay` to suppress the problem.

**Limitations:** GPU driver timeouts cannot be reliably caught or recovered from inside Rust/WGPU if the OS or entire computer is already stalled. This profile reduces potentially risky work; it does not guarantee avoidance of a driver crash.

## Português (Brasil)

Investigação de congelamentos completos em placas NVIDIA GT 730 e GTX 750/750 Ti, incluindo relatos com `LiveKernelEvent 141` (`VIDEO_ENGINE_TIMEOUT_DETECTED`, envolvendo `nvlddmkm.sys`). **É uma mitigação, não uma correção comprovada para o timeout da GPU**. Um evento 141 também pode envolver driver, hardware, temperatura, alimentação ou a carga processada.

**Mudança (somente Windows):** quando o adaptador for reconhecido como uma dessas placas, aplicar MSAA 1x, anisotropia máxima 2x, SSAO desligado, sombras até 1024, sem ray queries experimentais, sem recurso opcional de cache de pipelines e com limites de dispositivo mais conservadores. Texturas BC/DXT continuam comprimidas quando suportadas, para evitar pressão adicional de VRAM. Não altera configuração global, protocolo, timeout de TDR do Windows nem outras GPUs.

**Para comparação:** `OMSI_NVIDIA_LEGACY_FULL_GPU=1` restaura o caminho gráfico completo apenas em testes controlados, quando o driver for estável. **Não orientar usuários que têm travamentos completos do PC a ativar essa variável de forma casual.**

**Validação:** testes unitários e compilação Release Windows; abertura do menu e jogo em mapa leve e pesado; comparação com placa NVIDIA moderna; registro de modelo, driver, `game.log`, Monitor de Confiabilidade do Windows e arquivos de `C:\Windows\LiveKernelReports\WATCHDOG` se existirem. Depois de um travamento do PC, interromper os testes nessa build até analisar as evidências. Nunca aumentar `TdrDelay` para mascarar a falha.

**Limite:** se o driver ou Windows travou, o Rust/WGPU não consegue necessariamente recuperar o sistema. A mudança reduz operações potencialmente problemáticas; não promete evitar todos os travamentos.
