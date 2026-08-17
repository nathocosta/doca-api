# 🔧 Doca-API - Backend

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0)
[![CI](https://github.com/nathocosta/doca-api/actions/workflows/ci.yml/badge.svg)](https://github.com/nathocosta/doca-api/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![Deploy-Render](https://img.shields.io/badge/Deploy-Render-purple.svg)](https://render.com)

API REST robusta e assíncrona escrita em **Rust** para processamento, manipulação e segurança de documentos PDF de alta performance.

---

## 🚀 Versão Hospedada
A API em produção está implantada em:
**`https://doca-api.onrender.com`**

---

## 📊 Endpoints

Todos os endpoints que processam arquivos aceitam dados via formulário `multipart/form-data` (campo `files`).

| Método | Rota | Descrição | Parâmetros Form |
| :--- | :--- | :--- | :--- |
| `GET` | `/health` | Verifica a saúde do servidor. | N/A |
| `GET` | `/uptime` | Retorna o tempo de atividade da API. | N/A |
| `POST` | `/api/merge` | Junta múltiplos arquivos PDF em um único arquivo. | `files` (múltiplos PDFs) |
| `POST` | `/api/split` | Extrai páginas específicas de um PDF. | `files` (1 PDF), `ranges` (Ex: "1-3, 5") |
| `POST` | `/api/rotate` | Rotaciona as páginas de um PDF. | `files` (1 PDF), `angle` (90, 180, 270) |
| `POST` | `/api/img-to-pdf` | Converte imagens (PNG, JPG, JPEG) em um PDF. | `files` (múltiplas imagens) |
| `POST` | `/api/compress` | Otimiza e compacta streams de dados de um PDF. | `files` (1 PDF) |
| `POST` | `/api/docx-to-pdf` | Converte um arquivo do Microsoft Word (`.docx`) em PDF. | `files` (1 `.docx`) |
| `POST` | `/api/protect` | Protege um PDF com senha usando criptografia AES-128. | `files` (1 PDF), `password` (4-128 caracteres) |

---

## 🛠️ Tecnologias
*   **Rust** (Linguagem principal)
*   **Axum** (Framework web seguro e assíncrono construído sobre a stack hyper)
*   **Tokio** (Runtine assíncrono para I/O sem bloqueio)
*   **lopdf & printpdf** (Manipulação e estruturação de objetos e streams binários PDF)
*   **office2pdf** (Conversão nativa de documentos OpenXML `.docx` em PDF usando Typst)
*   **zeroize** (Limpeza segura e automática de dados de senhas sensíveis da memória RAM)

---

## 🔒 Medidas de Segurança
*   **Rate Limiting Dinâmico:** Limite padrão de 30 requisições por minuto por IP (e limite exclusivo de 15 requisições por minuto por IP no endpoint `/api/protect`).
*   **Concorrência Controlada:** Controle de concorrência por meio de semáforos (`tokio::sync::Semaphore`) limitando a 6 operações gerais de PDF e a 2 operações de criptografia de senhas em paralelo para proteger a memória Heap contra gargalos ou ataques DoS.
*   **Zeroização de Senhas:** Sanitização segura de strings de senhas (`zeroize::Zeroize`) após o uso nas operações de proteção de PDF.
*   **CORS Restrito:** Políticas restritivas aplicadas às origens do GitHub Pages e localhost.

---

## 🔧 Variáveis de Ambiente
*   `PORT`: Define a porta onde a API irá escutar (Padrão: `3000`).

---

## 🚀 Deploy Local

### Pré-requisitos
Certifique-se de ter instalado o compilador **Rust** e o gerenciador **Cargo** (Rust 1.70 ou superior).

1. Clone o repositório da API:
   ```bash
   git clone https://github.com/nathocosta/doca-api.git
   ```
2. Inicie o servidor em modo de desenvolvimento:
   ```bash
   cargo run
   ```
3. O servidor estará ativo em `http://localhost:3000`.

---

## 📦 Deploy no Render
Este projeto está estruturado para rodar perfeitamente no plano gratuito do Render. 

1. Crie um novo **Web Service** no Render e conecte o repositório.
2. Defina os seguintes parâmetros de Build:
   *   **Runtime:** `Rust`
   *   **Build Command:** `cargo build --release`
   *   **Start Command:** `./target/release/doca-api`
3. Defina a variável de ambiente `PORT` caso necessário.

---

## 🤝 Como Contribuir
Consulte o arquivo [CONTRIBUTING.md](CONTRIBUTING.md) para obter diretrizes detalhadas sobre submissão de bugs e pull requests.

---

## 📜 Licença
Distribuído sob a licença **AGPL-3.0** (GNU Affero General Public License v3). Consulte o arquivo [LICENSE](LICENSE) para obter mais informações.
