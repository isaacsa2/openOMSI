# Presets por modo gráfico

Vanilla+ continua sendo o padrão. Escolha primeiro o modo gráfico e depois Baixo, Médio, Alto ou Ultra. Os quatro níveis são ajustados para o modo escolhido e não mudam esse modo. Launcher e menu do jogo usam a mesma definição. Reinicie para aplicar as opções que dependem da criação do renderer.

| Modo | Baixo: MSAA / escala | Médio: MSAA / escala | Alto: MSAA / escala | Ultra: MSAA / escala |
| --- | --- | --- | --- | --- |
| Vanilla, como OMSI 2 | Desligado / 75% | 2× / automática | 4× / automática | 4× / automática |
| Vanilla+ — padrão | Desligado / 75% | 2× / automática | 4× / automática | 4× / automática |
| Enhanced | Desligado / 67% | Desligado / 85% | 2× / automática | 4× / automática |
| Enhanced+ | Desligado / 50% | Desligado / 67% | Desligado / 85% | 2× / automática |

Vanilla não ativa sombras de sol, SSAO ou textura de detalhe. Vanilla+ e Enhanced ativam sombras de sol a partir de Médio, SSAO a partir de Alto e detalhe/reflexos a partir de Médio. Enhanced+ mantém sombras, oclusão e reflexos que pertencem ao modo de ray tracing, mesmo em Baixo. Escolher um nível nunca ativa Enhanced+; o usuário precisa escolher esse modo explicitamente.

| Opção | Baixo | Médio | Alto | Ultra |
| --- | --- | --- | --- | --- |
| Visão / objetos em Vanilla e Vanilla+ | 600 / 500 m | 900 / 750 m | Automático / automático | 2000 / 1500 m |
| Visão / objetos em Enhanced e Enhanced+ | 600 / 500 m | 900 / 750 m | 1200 / 900 m | 2000 / 1500 m |
| Filtragem anisotrópica | 2× | 4× | 8× | 8× |
| Tamanho de shadow map, exceto Enhanced+ | 1024 | 1024 | 2048 | 4096 |
| Tamanho de shadow map em Enhanced+ | 1024 | 1024 | 2048 | 2048 |
| Espelhos, exceto Enhanced+ | 128 econômico | 256 econômico | 256 completo | 512 completo |
| Espelhos em Enhanced+ | 128 econômico | 128 econômico | 256 econômico | 512 completo |
| Nuvens e movimento das árvores | Desligados | Ligados | Ligados | Ligados |

Shadow map é um parâmetro de recursos, não uma indicação de que as sombras estejam ligadas em Vanilla ou em Baixo. Objetos pequenos usam os cortes 0,03 / 0,02 / 0,013 / 0,005 conforme o nível. Escala automática segue a política existente do renderer, não promete uma resolução ou FPS fixos.

Todos os níveis usam memória de textura automática. Não alteram a API gráfica escolhida, resolução da janela, V-sync, limite de FPS, controle, idioma ou densidade de passageiros/tráfego. As configurações existentes e os perfis personalizados continuam utilizáveis.

Os limites por GPU/formato, o caminho seguro de Android/OpenGL e o fallback de Enhanced+ para Enhanced sem ray queries continuam sendo os já existentes no renderer. Presets ajudam a reduzir carga e mantêm essas proteções; não há garantia de funcionamento em todo driver, mapa ou mod sem teste no hardware.

## Teste

1. No launcher e dentro do jogo, escolha cada modo e aplique os quatro níveis. O modo escolhido deve permanecer.
2. Salve e reinicie: o nome do preset deve ser reconhecido. Altere uma opção manualmente e confira a indicação de personalizado.
3. Compare Baixo/Médio no mesmo local com as opções anteriores, informando FPS, resolução, GPU e mapa. Teste o backend que funciona no aparelho; o preset não muda a API.
4. Em Vanilla, confira que efeitos exclusivos dos outros modos ficam desligados. Em Enhanced+, confira o comportamento com e sem suporte de ray tracing e guarde o log.
5. Reteste configurações já salvas e perfis personalizados, verificando que idioma, controles e densidade não mudaram.
