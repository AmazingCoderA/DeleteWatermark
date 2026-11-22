//! Встроенная веб-страница: лежит прямо в бинарнике, без внешних файлов.
pub const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="ru">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Удалитель Водяных Знаков v8.0 (Rust)</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 20px;
        }
        .container {
            max-width: 1000px;
            margin: 0 auto;
            background: white;
            border-radius: 20px;
            padding: 40px;
            box-shadow: 0 20px 60px rgba(0,0,0,0.3);
        }
        h1 { color: #667eea; text-align: center; margin-bottom: 10px; font-size: 2em; }
        .info {
            text-align: center; color: #666; margin-bottom: 30px;
            padding: 15px; background: #f8f9fa; border-radius: 10px;
        }
        .upload-zone {
            border: 3px dashed #ccc; border-radius: 15px; padding: 60px 20px;
            text-align: center; cursor: pointer; transition: all 0.3s; background: #fafafa;
        }
        .upload-zone:hover, .upload-zone.dragover {
            border-color: #667eea; background: #f0f4ff; transform: scale(1.02);
        }
        .upload-zone p { font-size: 1.2em; color: #666; margin: 10px 0; }
        .upload-zone strong { color: #667eea; }
        input[type="file"] { display: none; }
        .results { margin-top: 40px; }
        .results h2 { color: #333; margin-bottom: 20px; }
        .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 20px; }
        .card {
            border: 1px solid #e0e0e0; border-radius: 10px; padding: 15px;
            text-align: center; background: white; box-shadow: 0 2px 10px rgba(0,0,0,0.1);
            transition: transform 0.2s;
        }
        .card:hover { transform: translateY(-5px); box-shadow: 0 4px 20px rgba(0,0,0,0.15); }
        .card img { max-width: 100%; border-radius: 8px; margin-bottom: 10px; }
        .card p { font-weight: bold; margin-bottom: 10px; word-break: break-word; }
        .card a {
            display: inline-block; background: #667eea; color: white; padding: 10px 20px;
            border-radius: 8px; text-decoration: none; transition: background 0.2s;
        }
        .card a:hover { background: #5568d3; }
        .error { color: #d32f2f; font-size: 0.9em; }
        .progress { display: none; margin-top: 20px; text-align: center; }
        .spinner {
            border: 4px solid #f3f3f3; border-top: 4px solid #667eea;
            border-radius: 50%; width: 50px; height: 50px;
            animation: spin 1s linear infinite; margin: 20px auto;
        }
        @keyframes spin { 0% { transform: rotate(0deg); } 100% { transform: rotate(360deg); } }
    </style>
</head>
<body>
    <div class="container">
        <h1>🎨 Удалитель Водяных Знаков v8.0 <small>(Rust)</small></h1>
        <div class="info">
            <p><strong>Загружено образцов:</strong> {{ templates_count }}</p>
            <p>Перетащите файлы или нажмите на область ниже для выбора</p>
        </div>

        <div class="upload-zone" id="upload-zone">
            <p>📁 <strong>Перетащите изображения сюда</strong></p>
            <p>или нажмите для выбора файлов</p>
            <input type="file" id="file-input" multiple accept="image/png,image/jpeg,image/bmp,image/tiff">
        </div>

        <div class="progress" id="progress">
            <div class="spinner"></div>
            <p>Обработка изображений...</p>
        </div>

        <div class="results" id="results"></div>
    </div>

    <script>
        const uploadZone = document.getElementById('upload-zone');
        const fileInput = document.getElementById('file-input');
        const progress = document.getElementById('progress');
        const results = document.getElementById('results');

        uploadZone.addEventListener('click', () => fileInput.click());

        ['dragenter', 'dragover', 'dragleave', 'drop'].forEach(event => {
            uploadZone.addEventListener(event, e => { e.preventDefault(); e.stopPropagation(); });
        });
        ['dragenter', 'dragover'].forEach(event => {
            uploadZone.addEventListener(event, () => uploadZone.classList.add('dragover'));
        });
        ['dragleave', 'drop'].forEach(event => {
            uploadZone.addEventListener(event, () => uploadZone.classList.remove('dragover'));
        });
        uploadZone.addEventListener('drop', e => {
            if (e.dataTransfer.files.length > 0) uploadFiles(e.dataTransfer.files);
        });
        fileInput.addEventListener('change', () => {
            if (fileInput.files.length > 0) uploadFiles(fileInput.files);
        });

        async function uploadFiles(files) {
            const formData = new FormData();
            for (let file of files) formData.append('files', file);
            progress.style.display = 'block';
            results.innerHTML = '';
            try {
                const response = await fetch('/process', { method: 'POST', body: formData });
                const data = await response.json();
                displayResults(data);
            } catch (error) {
                results.innerHTML = '<p class="error">Ошибка при загрузке файлов</p>';
            } finally {
                progress.style.display = 'none';
            }
        }

        function esc(s) {
            return String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
        }

        function displayResults(data) {
            if (!data || data.length === 0) {
                results.innerHTML = '<p>Нет результатов</p>';
                return;
            }
            let html = '<h2>Результаты обработки:</h2><div class="grid">';
            data.forEach(item => {
                html += '<div class="card">';
                html += `<p>${esc(item.original)}</p>`;
                if (item.success) {
                    html += `<img src="data:image/png;base64,${item.image_b64}" alt="Результат">`;
                    html += `<a href="/download/${encodeURIComponent(item.filename)}" download="${esc(item.download_name)}">Скачать</a>`;
                } else {
                    html += `<p class="error">Ошибка: ${esc(item.message)}</p>`;
                }
                html += '</div>';
            });
            results.innerHTML = html + '</div>';
        }
    </script>
</body>
</html>
"##;
