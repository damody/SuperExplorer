# -*- coding: utf-8 -*-
from pathlib import Path
import re

root = Path(r"D:\SuperExplorer\crates\explorer-i18n\locales")

menus = {
    "en": {
        "downloads": "   ⇩  Run log",
        "block": """run-log-library = Run log
run-log-missing = File moved or missing
run-log-opens = Opened { $count } times
weekday-sun = Sunday
weekday-mon = Monday
weekday-tue = Tuesday
weekday-wed = Wednesday
weekday-thu = Thursday
weekday-fri = Friday
weekday-sat = Saturday
""",
    },
    "zh-TW": {
        "downloads": "   ⇩  運行記錄",
        "block": """run-log-library = 運行記錄
run-log-missing = 檔案已刪除或移動
run-log-opens = 已開啟 { $count } 次
weekday-sun = 星期日
weekday-mon = 星期一
weekday-tue = 星期二
weekday-wed = 星期三
weekday-thu = 星期四
weekday-fri = 星期五
weekday-sat = 星期六
""",
    },
    "zh-CN": {
        "downloads": "⇩  运行记录",
        "block": """run-log-library = 运行记录
run-log-missing = 文件已删除或移动
run-log-opens = 已打开 { $count } 次
weekday-sun = 星期日
weekday-mon = 星期一
weekday-tue = 星期二
weekday-wed = 星期三
weekday-thu = 星期四
weekday-fri = 星期五
weekday-sat = 星期六
""",
    },
    "ja": {
        "downloads": "   ⇩  実行履歴",
        "block": """run-log-library = 実行履歴
run-log-missing = ファイルは移動または削除されました
run-log-opens = { $count } 回開きました
weekday-sun = 日曜日
weekday-mon = 月曜日
weekday-tue = 火曜日
weekday-wed = 水曜日
weekday-thu = 木曜日
weekday-fri = 金曜日
weekday-sat = 土曜日
""",
    },
    "ko": {
        "downloads": "   ⇩  실행 기록",
        "block": """run-log-library = 실행 기록
run-log-missing = 파일이 이동되었거나 없습니다
run-log-opens = { $count }번 열림
weekday-sun = 일요일
weekday-mon = 월요일
weekday-tue = 화요일
weekday-wed = 수요일
weekday-thu = 목요일
weekday-fri = 금요일
weekday-sat = 토요일
""",
    },
    "de": {
        "downloads": "   ⇩  Ausführungsverlauf",
        "block": """run-log-library = Ausführungsverlauf
run-log-missing = Datei verschoben oder fehlt
run-log-opens = { $count } Mal geöffnet
weekday-sun = Sonntag
weekday-mon = Montag
weekday-tue = Dienstag
weekday-wed = Mittwoch
weekday-thu = Donnerstag
weekday-fri = Freitag
weekday-sat = Samstag
""",
    },
    "fr": {
        "downloads": "   ⇩  Journal d’exécution",
        "block": """run-log-library = Journal d’exécution
run-log-missing = Fichier déplacé ou manquant
run-log-opens = Ouvert { $count } fois
weekday-sun = dimanche
weekday-mon = lundi
weekday-tue = mardi
weekday-wed = mercredi
weekday-thu = jeudi
weekday-fri = vendredi
weekday-sat = samedi
""",
    },
    "es-ES": {
        "downloads": "   ⇩  Registro de ejecución",
        "block": """run-log-library = Registro de ejecución
run-log-missing = El archivo se movió o falta
run-log-opens = Abierto { $count } veces
weekday-sun = domingo
weekday-mon = lunes
weekday-tue = martes
weekday-wed = miércoles
weekday-thu = jueves
weekday-fri = viernes
weekday-sat = sábado
""",
    },
    "es-419": {
        "downloads": "   ⇩  Registro de ejecución",
        "block": """run-log-library = Registro de ejecución
run-log-missing = El archivo se movió o falta
run-log-opens = Abierto { $count } veces
weekday-sun = domingo
weekday-mon = lunes
weekday-tue = martes
weekday-wed = miércoles
weekday-thu = jueves
weekday-fri = viernes
weekday-sat = sábado
""",
    },
    "it": {
        "downloads": "   ⇩  Registro esecuzioni",
        "block": """run-log-library = Registro esecuzioni
run-log-missing = File spostato o mancante
run-log-opens = Aperto { $count } volte
weekday-sun = domenica
weekday-mon = lunedì
weekday-tue = martedì
weekday-wed = mercoledì
weekday-thu = giovedì
weekday-fri = venerdì
weekday-sat = sabato
""",
    },
    "pt-BR": {
        "downloads": "   ⇩  Registro de execução",
        "block": """run-log-library = Registro de execução
run-log-missing = Arquivo movido ou ausente
run-log-opens = Aberto { $count } vezes
weekday-sun = domingo
weekday-mon = segunda-feira
weekday-tue = terça-feira
weekday-wed = quarta-feira
weekday-thu = quinta-feira
weekday-fri = sexta-feira
weekday-sat = sábado
""",
    },
    "pt-PT": {
        "downloads": "   ⇩  Registo de execução",
        "block": """run-log-library = Registo de execução
run-log-missing = Ficheiro movido ou em falta
run-log-opens = Aberto { $count } vezes
weekday-sun = domingo
weekday-mon = segunda-feira
weekday-tue = terça-feira
weekday-wed = quarta-feira
weekday-thu = quinta-feira
weekday-fri = sexta-feira
weekday-sat = sábado
""",
    },
    "ru": {
        "downloads": "   ⇩  Журнал запуска",
        "block": """run-log-library = Журнал запуска
run-log-missing = Файл перемещён или отсутствует
run-log-opens = Открыто { $count } раз
weekday-sun = воскресенье
weekday-mon = понедельник
weekday-tue = вторник
weekday-wed = среда
weekday-thu = четверг
weekday-fri = пятница
weekday-sat = суббота
""",
    },
    "uk": {
        "downloads": "   ⇩  Журнал запуску",
        "block": """run-log-library = Журнал запуску
run-log-missing = Файл переміщено або відсутній
run-log-opens = Відкрито { $count } разів
weekday-sun = неділя
weekday-mon = понеділок
weekday-tue = вівторок
weekday-wed = середа
weekday-thu = четвер
weekday-fri = п’ятниця
weekday-sat = субота
""",
    },
    "pl": {
        "downloads": "   ⇩  Dziennik uruchomień",
        "block": """run-log-library = Dziennik uruchomień
run-log-missing = Plik przeniesiono lub usunięto
run-log-opens = Otwarto { $count } razy
weekday-sun = niedziela
weekday-mon = poniedziałek
weekday-tue = wtorek
weekday-wed = środa
weekday-thu = czwartek
weekday-fri = piątek
weekday-sat = sobota
""",
    },
    "cs": {
        "downloads": "   ⇩  Záznam spuštění",
        "block": """run-log-library = Záznam spuštění
run-log-missing = Soubor byl přesunut nebo chybí
run-log-opens = Otevřeno { $count }krát
weekday-sun = neděle
weekday-mon = pondělí
weekday-tue = úterý
weekday-wed = středa
weekday-thu = čtvrtek
weekday-fri = pátek
weekday-sat = sobota
""",
    },
    "hu": {
        "downloads": "   ⇩  Futtatási napló",
        "block": """run-log-library = Futtatási napló
run-log-missing = A fájl áthelyezve vagy hiányzik
run-log-opens = { $count } alkalommal megnyitva
weekday-sun = vasárnap
weekday-mon = hétfő
weekday-tue = kedd
weekday-wed = szerda
weekday-thu = csütörtök
weekday-fri = péntek
weekday-sat = szombat
""",
    },
    "tr": {
        "downloads": "   ⇩  Çalıştırma kaydı",
        "block": """run-log-library = Çalıştırma kaydı
run-log-missing = Dosya taşındı veya eksik
run-log-opens = { $count } kez açıldı
weekday-sun = Pazar
weekday-mon = Pazartesi
weekday-tue = Salı
weekday-wed = Çarşamba
weekday-thu = Perşembe
weekday-fri = Cuma
weekday-sat = Cumartesi
""",
    },
    "th": {
        "downloads": "   ⇩  บันทึกการเปิดไฟล์",
        "block": """run-log-library = บันทึกการเปิดไฟล์
run-log-missing = ไฟล์ถูกย้ายหรือหายไป
run-log-opens = เปิดแล้ว { $count } ครั้ง
weekday-sun = วันอาทิตย์
weekday-mon = วันจันทร์
weekday-tue = วันอังคาร
weekday-wed = วันพุธ
weekday-thu = วันพฤหัสบดี
weekday-fri = วันศุกร์
weekday-sat = วันเสาร์
""",
    },
    "vi": {
        "downloads": "   ⇩  Nhật ký chạy",
        "block": """run-log-library = Nhật ký chạy
run-log-missing = Tập tin đã bị chuyển hoặc mất
run-log-opens = Đã mở { $count } lần
weekday-sun = Chủ nhật
weekday-mon = Thứ hai
weekday-tue = Thứ ba
weekday-wed = Thứ tư
weekday-thu = Thứ năm
weekday-fri = Thứ sáu
weekday-sat = Thứ bảy
""",
    },
}

status = {
    "en": """status-run-log-missing = The file no longer exists.
status-run-log-opened = Opened.
status-run-log-open-failed = Unable to open: { $error }
""",
    "zh-TW": """status-run-log-missing = 檔案已不存在，無法執行。
status-run-log-opened = 已執行。
status-run-log-open-failed = 無法執行：{ $error }
""",
    "zh-CN": """status-run-log-missing = 文件已不存在，无法运行。
status-run-log-opened = 已运行。
status-run-log-open-failed = 无法运行：{ $error }
""",
    "ja": """status-run-log-missing = ファイルは存在しないため実行できません。
status-run-log-opened = 実行しました。
status-run-log-open-failed = 実行できません: { $error }
""",
    "ko": """status-run-log-missing = 파일이 더 이상 없어 실행할 수 없습니다.
status-run-log-opened = 실행했습니다.
status-run-log-open-failed = 실행할 수 없음: { $error }
""",
    "de": """status-run-log-missing = Die Datei ist nicht mehr vorhanden.
status-run-log-opened = Geöffnet.
status-run-log-open-failed = Öffnen nicht möglich: { $error }
""",
    "fr": """status-run-log-missing = Le fichier n’existe plus.
status-run-log-opened = Ouvert.
status-run-log-open-failed = Impossible d’ouvrir : { $error }
""",
    "es-ES": """status-run-log-missing = El archivo ya no existe.
status-run-log-opened = Abierto.
status-run-log-open-failed = No se puede abrir: { $error }
""",
    "es-419": """status-run-log-missing = El archivo ya no existe.
status-run-log-opened = Abierto.
status-run-log-open-failed = No se puede abrir: { $error }
""",
    "it": """status-run-log-missing = Il file non esiste più.
status-run-log-opened = Aperto.
status-run-log-open-failed = Impossibile aprire: { $error }
""",
    "pt-BR": """status-run-log-missing = O arquivo não existe mais.
status-run-log-opened = Aberto.
status-run-log-open-failed = Não foi possível abrir: { $error }
""",
    "pt-PT": """status-run-log-missing = O ficheiro já não existe.
status-run-log-opened = Aberto.
status-run-log-open-failed = Não foi possível abrir: { $error }
""",
    "ru": """status-run-log-missing = Файл больше не существует.
status-run-log-opened = Открыто.
status-run-log-open-failed = Не удалось открыть: { $error }
""",
    "uk": """status-run-log-missing = Файл більше не існує.
status-run-log-opened = Відкрито.
status-run-log-open-failed = Не вдалося відкрити: { $error }
""",
    "pl": """status-run-log-missing = Plik już nie istnieje.
status-run-log-opened = Otwarto.
status-run-log-open-failed = Nie można otworzyć: { $error }
""",
    "cs": """status-run-log-missing = Soubor již neexistuje.
status-run-log-opened = Otevřeno.
status-run-log-open-failed = Nelze otevřít: { $error }
""",
    "hu": """status-run-log-missing = A fájl már nem létezik.
status-run-log-opened = Megnyitva.
status-run-log-open-failed = Nem nyitható meg: { $error }
""",
    "tr": """status-run-log-missing = Dosya artık yok.
status-run-log-opened = Açıldı.
status-run-log-open-failed = Açılamadı: { $error }
""",
    "th": """status-run-log-missing = ไฟล์ไม่มีอยู่แล้ว
status-run-log-opened = เปิดแล้ว
status-run-log-open-failed = ไม่สามารถเปิดได้: { $error }
""",
    "vi": """status-run-log-missing = Tập tin không còn tồn tại.
status-run-log-opened = Đã mở.
status-run-log-open-failed = Không thể mở: { $error }
""",
}

for locale, data in menus.items():
    path = root / locale / "menus.ftl"
    text = path.read_text(encoding="utf-8-sig")
    text, n = re.subn(
        r"^menu-downloads-items =.*$",
        f"menu-downloads-items ={data['downloads']}",
        text,
        count=1,
        flags=re.M,
    )
    if n != 1:
        raise SystemExit(f"{locale} menus: downloads replace {n}")
    if "run-log-library =" not in text:
        text, n = re.subn(
            r"^(history-older =.*)$",
            r"\1\n" + data["block"].rstrip() + "\n",
            text,
            count=1,
            flags=re.M,
        )
        if n != 1:
            raise SystemExit(f"{locale} menus: insert {n}")
    path.write_text(text, encoding="utf-8", newline="\n")

for locale, block in status.items():
    path = root / locale / "status.ftl"
    text = path.read_text(encoding="utf-8-sig")
    if "status-run-log-missing =" not in text:
        text, n = re.subn(
            r"^(status-bookmark-file-launcher-unavailable =.*)$",
            r"\1\n" + block.rstrip(),
            text,
            count=1,
            flags=re.M,
        )
        if n != 1:
            raise SystemExit(f"{locale} status: insert {n}")
        path.write_text(text, encoding="utf-8", newline="\n")

print("ok", len(menus), "locales")
