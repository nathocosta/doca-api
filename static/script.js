// --- Dark Mode Logic ---
const htmlElement = document.documentElement;
const darkModeToggle = document.getElementById('dark-mode-toggle');
const darkModeIcon = document.getElementById('dark-mode-icon');

// Function to apply theme
function applyTheme(isDark) {
    if (isDark) {
        htmlElement.classList.add('dark');
        darkModeIcon.textContent = 'light_mode';
        darkModeIcon.setAttribute('title', 'Ativar modo claro');
    } else {
        htmlElement.classList.remove('dark');
        darkModeIcon.textContent = 'dark_mode';
        darkModeIcon.setAttribute('title', 'Ativar modo escuro');
    }
}

// Initialize theme based on localStorage or system preferences
const storedTheme = localStorage.getItem('theme');
const systemPrefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
const initialIsDark = storedTheme === 'dark' || (!storedTheme && systemPrefersDark);
applyTheme(initialIsDark);

// Event listener for theme toggle button
darkModeToggle.addEventListener('click', () => {
    const isCurrentlyDark = htmlElement.classList.contains('dark');
    const newIsDark = !isCurrentlyDark;
    applyTheme(newIsDark);
    localStorage.setItem('theme', newIsDark ? 'dark' : 'light');
});


// --- Mascot Interactive Logic (Removida pois o mascote agora é estático) ---


// --- About Modal Logic ---
const aboutModal = document.getElementById('about-modal');

function openAboutModal() {
    aboutModal.classList.remove('hidden');
    // Allow layout repaint before transitioning
    setTimeout(() => {
        aboutModal.classList.add('modal-active');
    }, 10);
}

function closeAboutModal() {
    aboutModal.classList.remove('modal-active');
    // Wait for transition to finish before hiding element
    setTimeout(() => {
        aboutModal.classList.add('hidden');
    }, 300);
}

// Close on clicking backdrop
aboutModal.addEventListener('click', (e) => {
    if (e.target === aboutModal) {
        closeAboutModal();
    }
});


// --- State Management ---
let currentTool = '';
let selectedFiles = []; // Array of { id, file, name, size }
let fileCounter = 0;

const toolConfigs = {
    'merge': {
        title: 'Juntar PDFs',
        accept: '.pdf',
        prompt: 'Suporta múltiplos documentos PDF. O resultado manterá a ordem da lista.',
        minFiles: 2,
        maxFiles: 20,
        settingsId: null
    },
    'split': {
        title: 'Dividir PDF',
        accept: '.pdf',
        prompt: 'Suporta um único documento PDF.',
        minFiles: 1,
        maxFiles: 1,
        settingsId: 'settings-split'
    },
    'img-to-pdf': {
        title: 'Imagem para PDF',
        accept: '.png,.jpg,.jpeg',
        prompt: 'Suporta imagens PNG, JPG, JPEG. A ordem das páginas segue a ordem da lista.',
        minFiles: 1,
        maxFiles: 50,
        settingsId: null
    },
    'rotate': {
        title: 'Rotacionar PDF',
        accept: '.pdf',
        prompt: 'Suporta um único documento PDF.',
        minFiles: 1,
        maxFiles: 1,
        settingsId: 'settings-rotate'
    },
    'unlock': {
        title: 'Desbloquear PDF',
        accept: '.pdf',
        prompt: 'Suporta um único documento PDF protegido por senha.',
        minFiles: 1,
        maxFiles: 1,
        settingsId: 'settings-unlock'
    }
};

// UI Elements
const viewDashboard = document.getElementById('dashboard');
const viewWorkspace = document.getElementById('workspace');
const toolTitle = document.getElementById('tool-title');
const fileFormatPrompt = document.getElementById('file-format-prompt');
const fileInput = document.getElementById('file-input');
const dropZone = document.getElementById('drop-zone');
const fileListSection = document.getElementById('file-list-section');
const fileListContainer = document.getElementById('file-list');
const fileCountLabel = document.getElementById('file-count');
const btnConvert = document.getElementById('btn-convert');
const btnActionText = document.getElementById('btn-action-text');

// Overlay Elements
const statusOverlay = document.getElementById('status-overlay');
const stateProcessing = document.getElementById('status-processing-state');
const stateSuccess = document.getElementById('status-success-state');
const stateError = document.getElementById('status-error-state');
const btnDownload = document.getElementById('btn-download');
const errorMessageText = document.getElementById('error-message-text');

// Switch Views
function switchView(tool) {
    currentTool = tool;
    const config = toolConfigs[tool];
    
    // Clear state
    selectedFiles = [];
    fileCounter = 0;
    
    // Configure workspace UI
    toolTitle.textContent = config.title;
    fileFormatPrompt.textContent = config.prompt;
    fileInput.setAttribute('accept', config.accept);
    
    // Hide all tool-specific settings
    document.getElementById('settings-split').style.display = 'none';
    document.getElementById('settings-rotate').style.display = 'none';
    document.getElementById('settings-unlock').style.display = 'none';
    
    // Show specific settings if any
    if (config.settingsId) {
        document.getElementById(config.settingsId).style.display = 'flex';
    }
    
    // Update labels
    btnActionText.textContent = getActionButtonText(tool);
    
    // UI states
    fileListSection.classList.add('hidden');
    dropZone.classList.remove('hidden');
    
    // Toggle active classes
    viewDashboard.classList.add('hidden');
    viewWorkspace.classList.remove('hidden');
}

function goHome() {
    viewWorkspace.classList.add('hidden');
    viewDashboard.classList.remove('hidden');
    currentTool = '';
    selectedFiles = [];
}

// Generate Action Button Label
function getActionButtonText(tool) {
    switch (tool) {
        case 'merge': return 'Mesclar PDFs';
        case 'split': return 'Dividir PDF';
        case 'img-to-pdf': return 'Converter Imagens';
        case 'rotate': return 'Rotacionar Páginas';
        case 'unlock': return 'Remover Senha';
        default: return 'Processar Arquivos';
    }
}

// Drag and drop setup for main page
document.addEventListener('DOMContentLoaded', () => {
    if (!dropZone) return;
    
    ['dragenter', 'dragover'].forEach(eventName => {
        dropZone.addEventListener(eventName, (e) => {
            e.preventDefault();
            dropZone.classList.add('border-primary', 'bg-primary/5');
        }, false);
    });

    ['dragleave', 'drop'].forEach(eventName => {
        dropZone.addEventListener(eventName, (e) => {
            e.preventDefault();
            dropZone.classList.remove('border-primary', 'bg-primary/5');
        }, false);
    });

    dropZone.addEventListener('drop', (e) => {
        const dt = e.dataTransfer;
        const files = dt.files;
        handleFiles(files);
    });

    fileInput.addEventListener('change', (e) => {
        handleFiles(e.target.files);
    });
});

// Handle Uploaded Files
function handleFiles(files) {
    const config = toolConfigs[currentTool];
    const allowedExtensions = config.accept.split(',');
    
    if (config.maxFiles === 1) {
        selectedFiles = [];
    }

    for (let i = 0; i < files.length; i++) {
        const file = files[i];
        const ext = '.' + file.name.split('.').pop().toLowerCase();
        
        const isValidExtension = allowedExtensions.some(allowedExt => {
            if (allowedExt === '.jpg' || allowedExt === '.jpeg') {
                return ext === '.jpg' || ext === '.jpeg';
            }
            return allowedExt === ext;
        });

        if (!isValidExtension) {
            alert(`Tipo de arquivo não permitido para esta ferramenta. Esperado: ${config.accept}`);
            continue;
        }

        if (selectedFiles.length >= config.maxFiles) {
            alert(`Limite máximo de ${config.maxFiles} arquivo(s) atingido.`);
            break;
        }

        selectedFiles.push({
            id: fileCounter++,
            file: file,
            name: file.name,
            size: formatBytes(file.size)
        });
    }

    renderFileList();
    fileInput.value = '';
}

// Format Bytes
function formatBytes(bytes, decimals = 2) {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const dm = decimals < 0 ? 0 : decimals;
    const sizes = ['Bytes', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
}

// Render File List
function renderFileList() {
    fileListContainer.innerHTML = '';
    
    if (selectedFiles.length === 0) {
        fileListSection.classList.add('hidden');
        dropZone.classList.remove('hidden');
        return;
    }
    
    fileListSection.classList.remove('hidden');
    
    const config = toolConfigs[currentTool];
    
    if (selectedFiles.length >= config.maxFiles) {
        dropZone.classList.add('hidden');
    } else {
        dropZone.classList.remove('hidden');
    }
    
    fileCountLabel.textContent = selectedFiles.length;

    selectedFiles.forEach((fileObj, index) => {
        const fileCard = document.createElement('div');
        fileCard.className = 'flex items-center gap-sm p-sm bg-surface-container/50 dark:bg-zinc-800/40 rounded-2xl border border-surface-variant dark:border-zinc-800/80 justify-between';
        
        const isPdf = fileObj.name.toLowerCase().endsWith('.pdf');
        const iconName = isPdf ? 'picture_as_pdf' : 'image';
        
        let reorderButtonsHtml = '';
        if (config.maxFiles > 1) {
            reorderButtonsHtml = `
                <div class="flex flex-col gap-0.5">
                    <button onclick="moveFile(${index}, -1)" ${index === 0 ? 'disabled' : ''} class="text-on-surface-variant dark:text-zinc-500 hover:text-primary dark:hover:text-white disabled:opacity-30 cursor-pointer p-0.5 bg-transparent border-none">
                        <span class="material-symbols-outlined text-xs">keyboard_arrow_up</span>
                    </button>
                    <button onclick="moveFile(${index}, 1)" ${index === selectedFiles.length - 1 ? 'disabled' : ''} class="text-on-surface-variant dark:text-zinc-500 hover:text-primary dark:hover:text-white disabled:opacity-30 cursor-pointer p-0.5 bg-transparent border-none">
                        <span class="material-symbols-outlined text-xs">keyboard_arrow_down</span>
                    </button>
                </div>
            `;
        }

        fileCard.innerHTML = `
            <div class="flex items-center gap-xs overflow-hidden flex-grow">
                <span class="material-symbols-outlined text-primary dark:text-zinc-300">${iconName}</span>
                <div class="flex flex-col overflow-hidden">
                    <span class="text-sm font-semibold truncate text-on-surface dark:text-zinc-200" title="${fileObj.name}">${fileObj.name}</span>
                    <span class="text-xs text-on-surface-variant dark:text-zinc-400">${fileObj.size}</span>
                </div>
            </div>
            <div class="flex items-center gap-xs flex-shrink-0">
                ${reorderButtonsHtml}
                <button onclick="removeFile(${fileObj.id})" class="text-on-surface-variant dark:text-zinc-400 hover:text-error dark:hover:text-red-400 cursor-pointer p-1 rounded-full bg-transparent border-none">
                    <span class="material-symbols-outlined text-sm">close</span>
                </button>
            </div>
        `;
        fileListContainer.appendChild(fileCard);
    });
}

// Move Item manually
function moveFile(index, direction) {
    const targetIndex = index + direction;
    if (targetIndex < 0 || targetIndex >= selectedFiles.length) return;
    const temp = selectedFiles[index];
    selectedFiles[index] = selectedFiles[targetIndex];
    selectedFiles[targetIndex] = temp;
    renderFileList();
}

// Remove File
function removeFile(id) {
    selectedFiles = selectedFiles.filter(f => f.id !== id);
    renderFileList();
}

// Clear all
function clearFiles() {
    selectedFiles = [];
    renderFileList();
}

// Toggle password visibility
function togglePasswordVisibility(fieldId) {
    const field = document.getElementById(fieldId);
    const toggleBtn = field.nextElementSibling;
    if (field.type === 'password') {
        field.type = 'text';
        toggleBtn.textContent = 'Ocultar';
    } else {
        field.type = 'password';
        toggleBtn.textContent = 'Mostrar';
    }
}

// Toggle status overlay
function showStatusOverlay(state) {
    statusOverlay.classList.remove('hidden');
    setTimeout(() => {
        statusOverlay.classList.add('modal-active');
    }, 10);
    
    stateProcessing.classList.add('hidden');
    stateSuccess.classList.add('hidden');
    stateError.classList.add('hidden');
    
    if (state === 'processing') stateProcessing.classList.remove('hidden');
    else if (state === 'success') stateSuccess.classList.remove('hidden');
    else if (state === 'error') stateError.classList.remove('hidden');
}

function hideStatusOverlay() {
    statusOverlay.classList.remove('modal-active');
    setTimeout(() => {
        statusOverlay.classList.add('hidden');
    }, 300);
}

function resetWorkspace() {
    hideStatusOverlay();
    clearFiles();
}

// Validate Params
function validateParams() {
    const config = toolConfigs[currentTool];
    
    if (selectedFiles.length < config.minFiles) {
        alert(`Por favor, adicione pelo menos ${config.minFiles} arquivo(s) para esta ferramenta.`);
        return false;
    }

    if (currentTool === 'split') {
        const ranges = document.getElementById('split-ranges').value.trim();
        if (ranges !== "" && !/^[0-9\s,\-]+$/.test(ranges)) {
            alert('Intervalo inválido. Use números, vírgulas e hífen (Ex: "1-3, 5").');
            return false;
        }
    }

    if (currentTool === 'unlock') {
        const pwd = document.getElementById('unlock-password').value;
        if (!pwd || pwd.length < 1) {
            alert('Por favor, digite a senha necessária para desbloquear o PDF.');
            return false;
        }
    }

    return true;
}

// Send Files to API
async function processDocuments() {
    if (!validateParams()) return;
    
    showStatusOverlay('processing');
    
    const formData = new FormData();
    
    selectedFiles.forEach(fileObj => {
        formData.append('files', fileObj.file);
    });

    if (currentTool === 'split') {
        formData.append('ranges', document.getElementById('split-ranges').value.trim());
    } else if (currentTool === 'rotate') {
        formData.append('angle', document.getElementById('rotate-angle').value);
    } else if (currentTool === 'unlock') {
        formData.append('password', document.getElementById('unlock-password').value);
    }

    const endpoint = `/api/${currentTool}`;

    try {
        const response = await fetch(endpoint, {
            method: 'POST',
            body: formData
        });

        if (!response.ok) {
            let errorText = 'Erro no processamento interno do servidor.';
            try {
                const errData = await response.json();
                errorText = errData.error || errorText;
            } catch (e) {
                errorText = await response.text() || `Erro no servidor (${response.status})`;
            }
            throw new Error(errorText);
        }

        const blob = await response.blob();
        const downloadUrl = URL.createObjectURL(blob);
        
        btnDownload.href = downloadUrl;
        
        let downloadName = 'documento_processado.pdf';
        const contentDisposition = response.headers.get('Content-Disposition');
        if (contentDisposition) {
            const matches = /filename[^;=\n]*=((['"]).*?\2|[^;\n]*)/.exec(contentDisposition);
            if (matches != null && matches[1]) { 
                downloadName = matches[1].replace(/['"]/g, '');
            }
        } else {
            if (currentTool === 'merge') downloadName = 'documento_mesclado.pdf';
            else if (currentTool === 'split') downloadName = 'documento_dividido.pdf';
            else if (currentTool === 'img-to-pdf') downloadName = 'imagens_convertidas.pdf';
            else if (currentTool === 'rotate') downloadName = 'documento_rotacionado.pdf';
            else if (currentTool === 'unlock') downloadName = 'documento_desbloqueado.pdf';
        }
        
        btnDownload.setAttribute('download', downloadName);
        showStatusOverlay('success');
    } catch (error) {
        console.error('Erro de conversão:', error);
        errorMessageText.textContent = error.message;
        showStatusOverlay('error');
    }
}
