# Proton Bus converter — recovery in progress

This draft recovers [PR #220](https://github.com/isaacsa2/openOMSI/pull/220), originally developed in October 2026, on top of the official openOMSI `main` of 15 November 2026.

The historical implementation is preserved at `archive/pr220-legacy-20261013`. The refreshed PR does **not** overwrite current engine or launcher files with their October versions.

## Recovered and verified

- Restored the complete experimental `crates/omsi-proton/` source, synthetic tests and command-line examples, without including third-party assets.
- Registered `omsi-proton` in the current Cargo workspace, added the needed `flate2` dependency and refreshed `Cargo.lock`.
- Isolated the legacy OpenOMSI-only OI envelope writer/decoder in the converter crate, instead of changing the current engine's VFS before a dedicated review.
- Ran rustfmt on the recovered source. The size-budget and security-audit workflows passed in this recovery.
- **`cargo check -p omsi-proton --lib --examples` passed on 15 November 2026.** This validates the library and the CLI examples against current dependencies, *not* game compatibility.

## Not yet working

**This is not a functional in-game Proton Bus importer and must not be merged or advertised as such.** The present game does not have the recovered OI readers, launcher Mods integration, scene/navigation/AI/passenger/vehicle rendering or simulator hooks. Merely copying older versions of these files would discard many newer upstream changes.

The older `cargo test -p omsi-proton --lib` suite still does **not compile** on current main. It references `omsi_cfg::oi` and older `GlobalCfg.unscheduled_passengers` / `SceneryObject.switched_map_lights` interfaces. These failures are known compatibility work, **not passing tests**. Existing synthetic fixtures must be ported together with the runtime code rather than deleted or silently skipped.

## Integration checklist

- [x] Preserve the original branch and recover the converter source on current upstream main
- [x] Format the converter code and register it in the Cargo workspace
- [x] Regenerate the lockfile and compile the converter library and CLI examples
- [ ] Design and validate secure, bounded OI reading from disk, mounted archives and other modern VFS layers; prefer current loader APIs
- [ ] Restore opt-in local map/vehicle conversion in Mods, with staging, rollback, file budgets and canonical `[openomsi]\nallowConversion=1` validation on every entry point
- [ ] Reconcile the current map/vehicle/scene/navigation, collision, passengers, traffic, signal-light and vehicle-animation interfaces without regressing unrelated features
- [ ] Port existing unit/integration tests and add modern regression coverage (including malformed OI headers, sizes, source paths, licences and cancellation)
- [ ] Update translated UI, user documentation, format notes and test instructions
- [ ] Run full PR checks, security audit, targeted tests and platform builds against the final head
- [ ] Perform real map/bus visual and gameplay testing, with explicit creator-supplied test packages

The `allowConversion=1` declaration is editable text, **not verified proof of identity or redistribution rights**. Respect the author's attribution and licence. Do not bundle or redistribute another game's models, scripts, maps, textures or sounds without permission.

Original archived head: [22c8401](https://github.com/isaacsa2/openOMSI/commit/22c8401e5d6bd868a7ee4e5ce031204eaa49552e). The October check runs failed size-budget and security audit; those historical results do not describe the current head.

---

## Resumo em português

A PR #220 foi reaberta e sincronizada com a `main` oficial. O código do conversor Proton Bus, os exemplos e o `Cargo.lock` foram recuperados e atualizados; a **biblioteca e os exemplos compilam**. Ainda faltam integração com o jogo, correções nos testes antigos, validação de mapas/ônibus reais e revisão completa. A PR continua em **rascunho**, sem merge ou alteração da `main`.
