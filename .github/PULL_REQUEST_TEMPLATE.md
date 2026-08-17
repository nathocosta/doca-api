## Descrição
Por favor, inclua um resumo das alterações na API e qual problema foi resolvido. Inclua também motivações relevantes e contexto técnico.

Resolva a issue relacionada: # (número da issue)

## Tipo de Alteração
Marque a opção que se aplica:
- [ ] Correção de bug (bug fix que não quebra compilação ou rotas existentes)
- [ ] Novo endpoint / funcionalidade (feature na API)
- [ ] Alteração de quebra (breaking change - alteração nos esquemas de entrada/saída de rotas existentes)
- [ ] Refatoração / Melhoria de performance e compilação

## Como foi testado?
Descreva os testes executados.
- [ ] Executei os testes unitários integrados: `cargo test`
- [ ] Testei manualmente a integração do endpoint com o frontend

## Checklist:
- [ ] Meu código segue as diretrizes de estilo do Rust (`cargo fmt` e `cargo clippy`)
- [ ] Eu fiz uma auto-revisão do meu próprio código
- [ ] Adicionei testes unitários adequados para cobrir as novas rotas/regras
- [ ] Minhas alterações não geraram novos alertas ou avisos de compilação
- [ ] Criei logs e validações adequadas para novos payloads
- [ ] Usei a zeroização segura (`zeroize`) caso tenha processado dados sensíveis
