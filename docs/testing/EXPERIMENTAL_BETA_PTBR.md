# Pacote experimental — 08/10/2026

Base oficial: `5f409baf5a3cd8d57874f08f2ab0a7ea00d9aafe`, main 0.2.17 com a modularização oficial.
Pacote: [fork PR #106](https://github.com/isaacsa2/openOMSI/pull/106), branch `test/ai-passengers-traffic-0.2`.

## Correções aceitas upstream

O mantenedor aplicou estas oito PRs por squash e fechou as PRs originais. O campo `merged_at` das PRs está vazio, mas os comentários do mantenedor e os commits da main confirmam a incorporação. Sete patches coincidem por `git patch-id --stable`; o de restrições tem as mesmas alterações, com contexto diferente porque foi aplicado depois das alterações de semáforos.

| Tema | Fork | Upstream | Commit na main |
| --- | --- | --- | --- |
| Limpeza de histórico de perfil | #98 | [#1727](https://github.com/openOMSI-Project/openOMSI/pull/1727) | `515f01de` |
| Visita exata à parada e lado da plataforma; centro da bounding box | #99 | [#1728](https://github.com/openOMSI-Project/openOMSI/pull/1728) | `7416db7a` |
| Estado de timetable na inicialização, repetição e streaming | #100 | [#1729](https://github.com/openOMSI-Project/openOMSI/pull/1729) | `5effce9f` |
| Identidade de parada usada pelos passageiros | #101 | [#1730](https://github.com/openOMSI-Project/openOMSI/pull/1730) | `a25fb85a` |
| Limite de spawn de passageiros | #103 | [#1731](https://github.com/openOMSI-Project/openOMSI/pull/1731) | `88e0dc9c` |
| Permissões de circulação e mudança de faixa | #104 | [#1732](https://github.com/openOMSI-Project/openOMSI/pull/1732) | `cb2d9329` |
| Semáforo de entrada sem identidade de scenery | #105 | [#1733](https://github.com/openOMSI-Project/openOMSI/pull/1733) | `28ee5cef` |
| Carros estacionados na verificação de faixa e merge | #107 | [#1734](https://github.com/openOMSI-Project/openOMSI/pull/1734) | `3d0efe46` |

Não reaplicamos esses patches sobre a main: o pacote usa a implementação oficial. A PR #102 do launcher permanece retirada. O changelog upstream #1742 foi recusado por política editorial: manter os resumos publicados e deixar os detalhes nos commits. A documentação histórica do fork #108 fica separada deste pacote.

## Novidades ainda no fork

| PR / branch | Alteração e causa confirmada no código | Limite da conclusão |
| --- | --- | --- |
| [#109](https://github.com/isaacsa2/openOMSI/pull/109) — `fix/articulated-door-state` | Usa a animação física correspondente a cada saída traseira, sem limitar o índice físico a 7; desativa a abertura presumida da AI quando qualquer porta fornece estado. Variáveis PAX explícitas continuam tendo prioridade. | Testes sintéticos; precisa testar o biarticulado do relato. Não altera arbitrariamente o contrato de variáveis PAX do OMSI. |
| [#110](https://github.com/isaacsa2/openOMSI/pull/110) — `fix/articulated-walk-entry` | Busca de ônibus perto do pedestre inclui módulos e posições reais de portas; antes dependia somente da origem do módulo frontal. | Corrige detecção perto da traseira. Não confirma nem resolve todo defeito de colisão do interior de um mod desconhecido. |
| [#111](https://github.com/isaacsa2/openOMSI/pull/111) — `fix/balanced-graphics-presets` | Launcher e jogo usam quatro níveis próprios para cada modo: Vanilla, Vanilla+ (padrão), Enhanced e Enhanced+. O nível preserva o modo; perfis Enhanced têm MSAA/escala mais conservadores. Memória de textura automática e configurações personalizadas preservadas. [Valores completos](GRAPHICS_PRESETS_PTBR.md). | Nenhum benchmark de FPS ou garantia para todos os PCs. Limites e fallbacks existentes do renderer preservados. Reiniciar para opções dependentes da inicialização. |
| [#112](https://github.com/isaacsa2/openOMSI/pull/112) — `feat/ingame-vehicle-hof-search` | Busca no jogo por modelo/pacote, HOF e destino; Enter confirma e Esc cancela; mantém a identidade da seleção. | Testes sintéticos de filtro; interação visual ainda precisa de teste. Letras/símbolos de linha já têm suporte na main e precisam validação com scripts reais. |
| Espera nos pontos — implementação oficial | A main incorporou a política de pontos de horário. A opção `ai_wait_timed_stops_only` é desligada por padrão; somente quando ativada os pontos comuns liberam após passageiros. | #113 substituída pelo upstream. Preservar o padrão oficial e testar ambos os estados da opção. |
| [`fix-duty-reselect-current-trip`](https://github.com/isaacsa2/openOMSI/tree/fix-duty-reselect-current-trip) | A seleção/reseleção de uma viagem e parada inicializa o turno com a hora atual do jogo, em vez do horário da primeira partida do tour. A escolha explícita de viagem/parada continua prevalecendo. | Integração sem conflito com busca e espera nos pontos. Testar seleção de uma viagem posterior à primeira e verificar horário, papel e destino após a seleção. |

## Diagnóstico dos 13 relatos originais

“Relatado pelos betatesters” não significa reproduzido nesta máquina. Não há mapas/veículos proprietários instalados neste ambiente.

| Problema | Reproduzido / evidência | Causa confirmada ou provável | Engine ou conteúdo/configuração | Subsistema | Risco / correção e relação |
| --- | --- | --- | --- | --- | --- |
| 1. `schedule_active` falso | Betatesters relataram letreiro, paradas e portas funcionando. Falhas de lifecycle cobertas por regressões sintéticas. | Inicialização e progressão de timetable corrigidas; espera em todo ponto era um problema distinto. | Falhas específicas da engine confirmadas; não explica automaticamente todo veículo. | `omsi-sim::timetable_run`, `ai_traffic::bus_service`, scripts | Médio: #100 já upstream; #113 substituída; testar a opção oficial. |
| 2. Embarque/desembarque AI | Testes dos usuários positivos em geral; relato de saída pela porta fechada no terceiro módulo. | Identidade de parada e heurísticas de portas têm falhas confirmadas no código. | Engine nesses casos; scripts do biarticulado ainda não examinados. | `omsi-sim::people`, passageiros, scripts PAX | Médio: #101 upstream, #109; testar estados de todas as portas. |
| 3. Portas esquerdas/BRT | Betatesters validaram plataformas elevadas dos dois lados; Recife e curvas não testados. Curitiba direita ainda falha. | Lado autorado, visita exata e centro da bounding box corrigidos. Causa residual de Curitiba não confirmada. | Correção geral da engine; geometry/asset residual desconhecido. | `bus_service.rs`, `schedule.rs`, bounding geometry | Médio: #99 upstream. Não forçar AI sempre para a direita. |
| 4. Rotatórias/interseções presas | Sem caso reproduzível ou log de crash. | Main já possui proteção de reservas; deadlock residual e crashes não isolados. | Indeterminado. | Reservas, blockers, occupancy, path progression | Alto: obter mapa/log; nenhuma correção nova de teleporte/despawn. |
| 5. Trajetória/calçada | Relacionado ao alinhamento; invasão da plataforma direita ainda relatada. | Possível relação com baia/geometry; steering independente não demonstrado. | Indeterminado no caso residual. | Track visit, offset, axles, articulação | Alto: testar curvas, veículos diferentes e plataforma exata. |
| 6. Destino ausente | Letreiro AI e N234 passaram nos testes dos usuários; símbolos incompletos. | Main e #100 cobrem timetable; não há HOF/Matrix do caso original para fechar diagnóstico. | Indeterminado para HOF/script específico. | HOF, strings, Matrix, timetable | Médio: preservar scripts; pedir veículo/HOF quando falhar. |
| 7. LAN veículo diferente | Nenhum novo teste nesta rodada. | Trabalho existente #74/#76 cobre identidade e resolução; nenhuma nova causa confirmada. | Não classificado novamente. | LAN identity, repaint, fleet, articulated sync | Alto: não duplicar; testar host/cliente e packs. |
| 8. Marcações somem/piscam | Não reproduzido. | Depth bias, precision, culling ou LOD permanecem hipóteses separadas. | Indeterminado. | Renderer, decals, materiais e bounds | Médio/alto: testcase; nenhum deslocamento global de Z. |
| 9. Runs após excluir perfil | Falha coberta por regressões de limpeza e proteção de outro perfil. | Referências persistidas e estado de perfil precisam invalidar na remoção. | Engine. | Launcher core, perfil e histórico | Médio: #98 upstream; testar com dois perfis temporários. |
| 10. Steam Deck/telas pequenas | Nenhum teste visual 1280×800 nesta rodada. | Main melhorou menu de pausa desktop e margens; isso não prova launcher inteiro corrigido. | UI da engine; configuração DPI também precisa teste. | Launcher/UI, scroll e layout | Baixo/médio: #102 retirada; testar launcher e menu separadamente. |
| 11. Textura de estrada trocada | Sem mapa/asset reproduzível. | Lookup, indexing, fallback e backend são hipóteses. | Indeterminado; não atribuir ao mapa sem evidência. | Content/material/texture/render | Médio: caso isolado necessário. |
| 12. Teto/modelo do ônibus | Sem o veículo específico nesta rodada. | UV, winding, O3D, material e caminho de textura precisam comparação. | Indeterminado; correções anteriores de winding não provam este caso. | O3D, mesh/material/content | Alto: não generalizar a partir de um modelo. |
| 13. Wires/objetos estranhos | Sem identificação reproduzível do objeto. | Spline/scenery/attachment/transform/culling não isolados. | Indeterminado. | Map/scenery/render | Médio: aguardar testcase identificado. |

Também permanece aberto o relato de zero tráfego em certos mapas: é necessário identificar o mapa, pool de tráfego, restrições e log de carregamento. Densidade descontrolada e ausência total de tráfego são problemas diferentes.

## Validação desta adaptação

- Base oficial 0.2.17. Passageiros e timetable foram adaptados aos módulos de simulação; renderização usa os novos pipelines e passes; a aplicação usa os estados tipados.
- Preservadas ANGLE, as opções novas de configuração, a correção oficial do teste Lua e o comportamento padrão da espera de AI.
- PRs temáticas permanecem isoladas; #106 é integração para teste e não uma proposta única para merge upstream.
- Os checks anteriores não comprovam esta base. A validação local e os novos resultados de CI devem ser conferidos no head atualizado de cada PR.
- Não houve benchmark com instalação original, execução de mapas/mods, nem validação física de Windows, Android, Steam Deck ou VR neste ambiente.

## Mensagem para os betatesters

Pessoal, atualizei o pacote experimental pra main atual do openOMSI, já com as mudanças da 0.2.17. Oito correções nossas entraram na versão oficial: passageiros, timetable, paradas, alinhamento de plataformas, restrições, semáforos, desvio de carros estacionados e limpeza de perfil.

O pacote acrescenta ajustes nas portas traseiras e na entrada a pé de articulados, na espera do ônibus AI nos pontos, presets gráficos mais leves e busca de veículos/HOF/destinos dentro do jogo. Também inclui um ajuste ao escolher ou reselecionar uma viagem do turno: usa a hora atual do jogo e mantém a viagem/parada escolhida.

Quando o build da plataforma terminar, usem o artifact da PR #106: https://github.com/isaacsa2/openOMSI/pull/106 . Extraiam numa pasta separada e confiram a versão que aparece no launcher. O download oficial pode aparecer antes ou depois deste pacote; os testes precisam identificar qual build foi usado.

Queria que vocês testassem principalmente:

- Passageiros com densidade normal e alta: quantidade nas paradas, embarque, desembarque e crescimento sem controle.
- Ônibus AI: viagem ativa, portas, letreiro, paradas e troca pra próxima viagem. Testem o padrão oficial e a opção `ai_wait_timed_stops_only`: com ela ativada, ponto comum libera depois dos passageiros; ponto de horário e final de linha mantêm a espera prevista.
- Biarticulados: portas do último módulo fechadas não devem permitir saída. Tentem entrar a pé pela traseira e avisem se a colisão do interior continuar falhando.
- Carros, táxis, caminhões e ônibus em faixas com restrições. Avisem também se algum veículo ficar preso ou se um mapa ficar sem tráfego.
- Semáforos: aproximação no vermelho, transições e cruzamentos seguidos. Principalmente pista dupla e faixa esquerda. Se alguém furar, gravem desde a aproximação.
- Portas direitas/esquerdas, plataformas centrais/BRT, articulados e paradas em curva. Retestem a plataforma direita de Curitiba; esse caso e o Recife ainda precisam confirmação.
- Escolham primeiro o modo gráfico e depois Low, Medium, High ou Ultra. O modo deve continuar o escolhido; Vanilla+ permanece padrão. Comparem Low/Medium no mesmo local, salvem e reiniciem, conferindo nome do preset e opções. Mandem FPS, placa de vídeo, resolução, API e mapa. Configuração salva não muda sozinha. Valores: docs/testing/GRAPHICS_PRESETS_PTBR.md.
- Busca de veículo, HOF e destino dentro do jogo: digitar, confirmar, apagar a busca e cancelar com Esc.
- Selecionar ou reselecionar uma viagem posterior à primeira do turno, sem mudar a hora do jogo: conferir viagem, parada, horário, papel e letreiro. Testem também a troca para a viagem seguinte.
- Launcher e menu de pausa no Steam Deck/telas pequenas e no desktop: campos, botões e rolagem.
- Exclusão de um perfil temporário: os runs dele devem sumir sem afetar outro perfil.
- Linhas com símbolos/letras, como -10, 10E e N41.

Se der problema, mandem versão do build, mapa e veículo com versões, local, horário, linha/tour, configurações de AI/passageiros, vídeo curto e log. Se aparecer uma quantidade de “milhões”, mostrem onde esse número aparece também. Para crashes com fila de tráfego, guardem o log logo depois do fechamento.

Os resultados anteriores não substituem os checks dos novos commits. A adaptação está sendo validada nesta base; ainda precisamos validar o comportamento nos mapas e veículos de vocês.
