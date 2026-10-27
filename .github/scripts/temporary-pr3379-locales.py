#!/usr/bin/env python3
# Temporary one-shot patch for openOMSI PR #3379; removed by the workflow that runs it.
from pathlib import Path
import json
import re

DATA = {"ca":["Desactiva els complements","Inicia la partida següent sense complements Lua ni complements antics d'OMSI, inclosos els dels vehicles. Les funcions dels complements no estaran disponibles. Els scripts normals dels autobusos continuaran funcionant."],"zh-tw":["停用外掛程式","下次啟動遊戲時不會載入 Lua 或舊版 OMSI 外掛程式，包括車輛外掛程式。外掛功能將無法使用。一般公車腳本仍會執行。"],"zh":["禁用插件","下次启动游戏时不加载 Lua 或旧版 OMSI 插件，包括车辆插件。插件功能将不可用。普通公交车脚本仍会运行。"],"nl":["Plug-ins uitschakelen","Start het volgende spel zonder Lua-plug-ins of oudere OMSI-plug-ins, waaronder voertuigplug-ins. Plug-infuncties zijn dan niet beschikbaar. Normale busscripts blijven werken."],"sv":["Inaktivera insticksprogram","Startar nästa spel utan Lua-insticksprogram eller äldre OMSI-insticksprogram, även för fordon. Insticksprogrammens funktioner är då inte tillgängliga. Vanliga busskript fortsätter att köras."],"sr":["Isključi dodatke","Pokreće sledeću igru bez Lua dodataka i starih OMSI dodataka, uključujući dodatke za vozila. Funkcije dodataka neće biti dostupne. Obične skripte autobusa nastavljaju da rade."],"ru":["Отключить плагины","Следующий запуск игры будет без плагинов Lua и старых плагинов OMSI, включая плагины транспорта. Функции плагинов будут недоступны. Обычные скрипты автобусов продолжат работать."],"ka":["პლაგინების გამორთვა","თამაშის შემდეგი გაშვება მოხდება Lua-სა და OMSI-ის ძველი პლაგინების გარეშე, მათ შორის სატრანსპორტო საშუალებების პლაგინების გარეშე. პლაგინების ფუნქციები მიუწვდომელი იქნება. ავტობუსის ჩვეულებრივი სკრიპტები კვლავ იმუშავებს."],"uk":["Вимкнути плагіни","Наступний запуск гри відбудеться без плагінів Lua та старих плагінів OMSI, зокрема плагінів транспорту. Функції плагінів будуть недоступні. Звичайні скрипти автобусів продовжать працювати."],"be":["Адключыць плагіны","Наступны запуск гульні будзе без плагінаў Lua і старых плагінаў OMSI, у тым ліку для транспартных сродкаў. Функцыі плагінаў будуць недаступныя. Звычайныя скрыпты аўтобусаў працягнуць працаваць."],"kk":["Плагиндерді өшіру","Ойын келесі жолы Lua және ескі OMSI плагиндерінсіз, соның ішінде көлік плагиндерінсіз іске қосылады. Плагин мүмкіндіктері қолжетімсіз болады. Автобустың қалыпты скрипттері жұмысын жалғастырады."],"pl":["Wyłącz wtyczki","Uruchamia następną grę bez wtyczek Lua i starszych wtyczek OMSI, w tym wtyczek pojazdów. Funkcje wtyczek będą niedostępne. Zwykłe skrypty autobusów nadal działają."],"cs":["Zakázat pluginy","Příští spuštění hry proběhne bez pluginů Lua a starších pluginů OMSI, včetně pluginů vozidel. Funkce pluginů nebudou dostupné. Běžné skripty autobusů budou nadále fungovat."],"hu":["Bővítmények letiltása","A következő játék Lua-bővítmények és régi OMSI-bővítmények nélkül indul, beleértve a járműbővítményeket is. A bővítmények funkciói nem lesznek elérhetők. A szokásos buszszkriptek tovább működnek."],"it":["Disattiva i plugin","Avvia la prossima partita senza plugin Lua o plugin OMSI precedenti, inclusi quelli dei veicoli. Le funzioni dei plugin non saranno disponibili. Gli script normali degli autobus continueranno a funzionare."],"tr":["Eklentileri devre dışı bırak","Sonraki oyunu, araç eklentileri de dahil olmak üzere Lua ve eski OMSI eklentileri olmadan başlatır. Eklenti özellikleri kullanılamaz. Normal otobüs betikleri çalışmaya devam eder."],"ja":["プラグインを無効にする","次回のゲーム起動時、車両用を含む Lua プラグインと旧式の OMSI プラグインを読み込みません。プラグインの機能は利用できなくなります。通常のバススクリプトは引き続き動作します。"],"ko":["플러그인 비활성화","다음 게임 실행 시 차량 플러그인을 포함한 Lua 및 기존 OMSI 플러그인을 불러오지 않습니다. 플러그인 기능을 사용할 수 없지만 일반 버스 스크립트는 계속 작동합니다."],"th":["ปิดใช้งานปลั๊กอิน","การเริ่มเกมครั้งถัดไปจะไม่โหลดปลั๊กอิน Lua หรือปลั๊กอิน OMSI แบบเก่า รวมถึงปลั๊กอินของยานพาหนะ ฟีเจอร์ของปลั๊กอินจะใช้ไม่ได้ แต่สคริปต์รถบัสทั่วไปยังทำงานตามปกติ"],"vi":["Tắt plugin","Lần khởi động trò chơi tiếp theo sẽ không tải plugin Lua hoặc plugin OMSI cũ, kể cả plugin của xe. Các tính năng của plugin sẽ không khả dụng. Script xe buýt thông thường vẫn chạy."],"id":["Nonaktifkan plugin","Memulai permainan berikutnya tanpa plugin Lua atau plugin OMSI lama, termasuk plugin kendaraan. Fitur plugin tidak tersedia. Skrip bus biasa tetap berjalan."],"ms":["Nyahdayakan pemalam","Memulakan permainan seterusnya tanpa pemalam Lua atau pemalam OMSI lama, termasuk pemalam kenderaan. Ciri pemalam tidak akan tersedia. Skrip bas biasa masih berjalan."],"tl":["Huwag paganahin ang mga plugin","Sisimulan ang susunod na laro nang walang mga Lua plugin o lumang OMSI plugin, kasama ang mga plugin ng sasakyan. Hindi magagamit ang mga feature ng plugin. Patuloy na gagana ang mga karaniwang bus script."],"hi":["प्लगइन बंद करें","अगला गेम Lua और पुराने OMSI प्लगइन के बिना शुरू होगा, जिनमें वाहन प्लगइन भी शामिल हैं। प्लगइन की सुविधाएँ उपलब्ध नहीं होंगी। सामान्य बस स्क्रिप्ट चलती रहेंगी।"]}
LOCALES = "es ca fr zh-tw pt pt-pt zh de nl sv sr ru ka uk be kk pl cs hu it tr ja ko th vi id ms tl hi".split()
p = Path("crates/omsi-app/locales/app.yml")
raw = p.read_bytes()
if b"\r\n" in raw:
    raise SystemExit("Unexpected CRLF in app.yml: refusing to change line endings")
src = raw.decode("utf-8")
label_key = '"Disable plugins":'
help_key = '"Starts the next game without Lua or legacy OMSI plugins, including vehicle plugins. Plugin features will be unavailable. Normal bus scripts still run.":'
end_key = '"Display and driver":'
a = src.index(label_key + "\n")
b = src.index(end_key + "\n", a)
labels, helps = {}, {}
dest = labels
for line in src[a:b].splitlines():
    if line == help_key:
        dest = helps
        continue
    found = re.fullmatch(r'  ([a-z-]+): ("(?:[^"\\]|\\.)*")', line)
    if found:
        dest[found.group(1)] = json.loads(found.group(2))
if len(labels) != 5 or len(helps) != 5 or (set(labels) != set(helps)):
    raise SystemExit(f"Unexpected pre-patch translations: {set(labels)} / {set(helps)}")
for lang, (label, help_text) in DATA.items():
    if lang in labels or lang in helps:
        raise SystemExit(f"Refusing to override existing locale: {lang}")
    labels[lang] = label
    helps[lang] = help_text
if set(labels) != set(LOCALES) or set(helps) != set(LOCALES):
    raise SystemExit("Locale coverage is not exactly 29 languages")
def lines_for(values):
    return "\n".join("  " + lang + ": " + json.dumps(values[lang], ensure_ascii=False) for lang in LOCALES)
replacement = label_key + "\n" + lines_for(labels) + "\n" + help_key + "\n" + lines_for(helps) + "\n\n"
updated = src[:a] + replacement + src[b:]
if updated[:a] != src[:a] or updated.endswith(src[b:]) is False:
    raise SystemExit("Unexpected change outside the new locale entries")
p.write_bytes(updated.encode("utf-8"))
print("Expanded both launcher keys to 29/29 locales, preserving all other app.yml contents")
