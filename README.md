# Умный удалитель водяных знаков v8.0 (Rust)

Порт скрипта `DeleteGdzSuper6.py` на Rust. Удаляет водяные знаки по **базе образцов** (шаблонов): ищет их на изображении в 25 масштабах, отсекает перекрытия NMS и закрашивает найденные области инпейнтингом.

**Главное отличие от Python-версии:** не нужен OpenCV, Python и `pip install`. Бинарник работает сам по себе на Windows, Linux, macOS и Android.

## Возможности

| Режим | Описание |
|-------|----------|
| `cli` | Пакетная обработка файлов/папок, прогресс и сводка ошибок |
| `web` | HTTP-сервер с drag&drop загрузкой, превью и скачиванием результата (как Flask-версия) |
| `gui` | Десктопное окно на egui: выбор файлов/папки, смена базы образцов, прогресс, отмена |

* Поиск: `TM_CCOEFF_NORMED` через FFT + интегральные изображения (эквивалент `cv2.matchTemplate`).
* **Точное удаление:** маска строится по форме самого знака (порог Оцу + порог от цвета фона + 1 px на сглаженные края), а не прямоугольником. Соседний текст рядом со знаком остаётся нетронутым и не размывается.
* Инпейнтинг в духе `INPAINT_TELEA` с переносом цвета вдоль изофот (по направлению градиента) и радиусом до 9 px.
* NMS с порогом перекрытия 0.3.
* Пирамида поиска: по умолчанию корреляция считается на половинном разрешении, затем кандидаты проверяются точным NCC в полном разрешении (в ~4 раза быстрее, результат тот же). `--pyramid 1` отключает пирамиду.
* Выбор базы образцов: флаг `--samples`, переменная `WATERMARK_SAMPLES` или кнопка в GUI.

## Быстрый старт

```bash
# Сборка (Rust 1.74+)
cargo build --release

# Обработка одного файла (результат — photo_cleaned.png рядом)
./target/release/delete-watermark --mode cli --input photo.png

# Целая папка в отдельную директорию
./target/release/delete-watermark --mode cli --input ./GDZ --output ./GDZ_cleaned

# Веб-интерфейс на порту 5000
./target/release/delete-watermark --mode web --port 5000
# Откройте http://127.0.0.1:5000 или http://<IP-в-ЛВС>:5000 с телефона

# Графический интерфейс (режим по умолчанию на десктопе)
./target/release/delete-watermark
```

На Windows пути и команды те же, executables называется `delete-watermark.exe`.

## Полный список флагов

```
--mode <cli|web|gui>     Режим запуска (по умолчанию gui на ПК, cli без GUI / на Android)
--samples <ПАПКА>       База образцов (папка с png/jpg/jpeg/bmp/tiff)
--input <ПУТЬ>          Файл или папка, можно повторять (режим cli)
--output <ПАПКА>        Куда сохранять результаты (режим cli)
--threshold 0.75        Порог совпадения шаблона
--precise               Максимальная аккуратность: точный поиск, порог 0.82, маска только по знаку
--nms 0.3               Порог перекрытия NMS
--scale-min 0.6         Минимальный масштаб шаблона
--scale-max 1.4         Максимальный масштаб шаблона
--scale-steps 25        Число шагов масштаба
--inpaint-radius 7      Радиус инпейнтинга
--pyramid <1..4>        1 = точно в полном размере, 2 = по умолчанию, 3 = быстрее
--list-samples          Только вывести список образцов
--host 0.0.0.0          Адрес веб-сервера
--port 5000             Порт веб-сервера
```

Пример с другой базой и «агрессивным» поиском:

```bash
delete-watermark --mode cli --samples ./assets/GDZ_SAMPLES/extra \
  --threshold 0.70 --pyramid 3 --input ./photo.jpg
```

## Максимальное качество

```bash
delete-watermark --mode cli --precise --input photo.png
```

`--precise` отключает пирамиду (поиск идёт в полном разрешении), поднимает порог
совпадения до 0.82 и оставляет маску строго по пикселям знака. Проверено на тесте,
где водяной знак стоит вплотную к другому тексту: соседние буквы не изменились
ни на один пиксель, а знак удалён полностью. Обычный режим (`--pyramid 2`) быстрее
и тоже использует ту же точную маску — отличается только скорость поиска.

## База образцов

В `assets/GDZ_SAMPLES/` уже лежит база, использованная в Python-проекте — 22 шаблона в корне и ещё 8 в подпапке `extra/` (в оригинале имя папки было нечитаемым, оно переименовано для кроссплатформенности). Это готовый пример: просто запустите программу, ничего копировать не нужно.

Порядок поиска базы:
1. `--samples <путь>`
2. `WATERMARK_SAMPLES=<путь>`
3. `<папка exe>/assets/GDZ_SAMPLES`
4. `<текущая папка>/assets/GDZ_SAMPLES`
5. `<папка exe>/GDZ_SAMPLES`, затем `<текущая папка>/GDZ_SAMPLES`

Чтобы сделать свою базу — положите в папку изображения водяных знаков (желательно вырезанные, на светлом фоне). Чем больше шаблонов, тем дольше поиск: сложность линейна от их количества.

## Автосборка

### Windows
```bat
build.bat
```
Результат: `dist\delete-watermark-v8.0-windows-x86_64.zip` (exe + assets + README).
 headless-вариант без GUI: `build.bat headless`.

### Linux / macOS
```bash
chmod +x build.sh
./build.sh            # с GUI
./build.sh headless   # только CLI + Web, без GTK/X11
```
Результат: `dist/delete-watermark-v8.0-<os>-<arch>.tar.gz`.

Для GUI на Linux нужны системные библиотеки:
```bash
sudo apt install libgtk-3-dev libx11-dev libxcb1-dev libxkbcommon-dev
```

### Android
Вариант 1 — кросс-компиляция с ПК:
```bash
cargo install cargo-ndk
export ANDROID_NDK_HOME=/path/to/android-ndk
./build_android.sh arm64-v8a armeabi-v7a
adb push dist/android /sdcard/delete-watermark
adb shell 'cd /sdcard/delete-watermark && sh run-web.sh'
```

Вариант 2 — сборка прямо на устройстве в Termux:
```bash
pkg install rust binutils git
bash build_termux.sh
delete-watermark-web        # веб-интерфейс на http://127.0.0.1:5000
```

GUI на Android не поддерживается — используйте `--mode web` (откройте страницу в браузере устройства) или `--mode cli`.

### APK (готовое приложение для Android)

В релизе лежит `delete-watermark-v8.0.apk` — установимое приложение: внутри WebView
открывается тот же интерфейс, а движок работает как встроенный Rust-веб-сервер.
База образцов из `assets/GDZ_SAMPLES/` копируется в хранилище приложения при первом запуске.

Собрать самому (нужны Android SDK с NDK и JDK; на CI уже есть):

```bash
bash android/build_apk.sh
# результат: build_apk/delete-watermark.apk
```

### GitHub Actions
`.github/workflows/build.yml` собирает Windows / Linux / macOS и Android-артефакты при каждом push и публикует их в Release при теге `v*`.

## Структура

```
assets/
  GDZ_SAMPLES/       база образцов (22 + 8 в extra/) — пример "из коробки"
  web/index.html     встроенная веб-страница (include_str!, без внешних файлов)
src/
  main.rs            CLI, выбор режима и настроек
  engine.rs          ядро: поиск, NMS, маска, инпейнтинг
  matcher.rs         FFT-корреляция + интегральные изображения
  image_ops.rs       grayscale, resize, downscale, inpaint, маски
  web.rs             HTTP/1.1-сервер, multipart, base64, download
  gui.rs             egui-окно (не собирается на Android)
build.bat / build.sh / build_android.sh / build_termux.sh
.github/workflows/build.yml
```

## Примечания по порту

* Цветовой перевод в серый делает `0.299R + 0.587G + 0.114B`, как OpenCV `COLOR_BGR2GRAY`; альфа игнорируется.
* Масштабы считаются как `np.linspace(0.6, 1.4, 25)`, ресайз шаблона — билинейный, как `cv2.resize`.
* NMS делит площадь пересечения на площадь **другого** бокса — как в оригинальном коде.
* Инпейнтинг — собственная реализация в духе Telea (распространение от границы маски с учётом градиента), поэтому пиксели могут немного отличаться от `cv2.inpaint`, но визуально результат тот же.
* Веб-сервер не требует Flask: это компактный HTTP-сервер на `std::net` с потоками на соединение, лимитом 100 МБ и встроенной страницей.

## Лицензия

MIT. База образцов взята из оригинального Python-проекта и включена как пример.
