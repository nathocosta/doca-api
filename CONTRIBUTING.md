# Guia de Contribuição - Doca API Backend

Obrigado pelo seu interesse em ajudar no desenvolvimento do backend do **Doca**! Este guia define as instruções técnicas para você construir, testar e enviar alterações.

---

## 🚀 Como Começar

1. **Faça um Fork** do repositório da API no GitHub.
2. **Clone** o seu fork localmente:
   ```bash
   git clone https://github.com/SEU_USUARIO/doca-api.git
   ```
3. **Crie uma Branch** para sua contribuição:
   ```bash
   git checkout -b feature/novo-endpoint
   ```

---

## 🛠️ Execução e Desenvolvimento Local

### Pré-requisitos
Instale a versão estável mais recente do compilador **Rust** via [rustup](https://rustup.rs).

1. Execute o servidor em modo de desenvolvimento local:
   ```bash
   cargo run
   ```
2. Por padrão, a API escutará na porta `3000`. Você pode alterar a porta configurando a variável de ambiente `PORT`.

---

## 📏 Estilo de Código e Boas Práticas

Para manter o código seguro e de fácil manutenção, siga estas diretrizes:

*   **Formatador:** Formate sempre o seu código antes de criar commits usando:
    *   `cargo fmt`
*   **Linter:** Verifique se não há avisos ou práticas ruins com:
    *   `cargo clippy`
*   **Segurança de Dados:** Se a sua melhoria processar senhas ou dados sensíveis na memória, use a trait `Zeroize` (`zeroize::Zeroize`) para garantir que os dados sejam sobrescritos na memória RAM imediatamente após o uso.
*   **Controle de Recursos:** Se a funcionalidade exigir muito processamento ou uso intenso de memória, utilize os semáforos compartilhados do `AppState` para evitar vazamentos de memória.

---

## 🧪 Rodando Testes

Seu código deve passar por todos os testes existentes. Se adicionar novas rotas ou regras em `src/pdf_ops.rs`, escreva testes unitários correspondentes no final do arquivo.

Execute os testes com:
```bash
cargo test
```

---

## 📥 Enviando o Pull Request

1. Faça o commit com mensagens descritivas seguindo a padronização:
   ```bash
   git commit -m "fix: corrige vazamento de memória na compressão"
   ```
2. Envie a branch para o seu repositório remoto:
   ```bash
   git push origin feature/novo-endpoint
   ```
3. Abra um **Pull Request** para a branch `main` do repositório original.
4. Preencha o template do PR detalhando a finalidade e como as mudanças foram validadas.
