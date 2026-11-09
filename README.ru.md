[English](README.md) | [Français](README.fr.md) | [Русский](README.ru.md)

<p align="center"><img src="assets/hammer-pro.png" width="96" alt="Hammer Pro logo: a white hammer on a black square"></p>

<a id="hammer-pro"></a>
# Hammer Pro

**Современный, быстрый редактор уровней для карт на движке Source, созданный в первую очередь для Garry's Mod.**

[Жить на французском языке](README.fr.md) | [Русский](README.ru.md) | [Документация](docs/ru/README.md) | [Документация на французском языке](docs/fr/README.md) | [Discord](https://discord.gg/eAEWWKRNAe)

Hammer Pro открывает и сохраняет стандартные файлы `.vmf`, поэтому вы можете использовать его вместе с Hammer и Hammer++. Он написан на Rust, рендерится с помощью GPU через wgpu, работает плавно на огромных картах и ​​компилирует карты с помощью собственного встроенного компилятора: VBSP, VVIS или VRAD не требуется.

Версия **1.2.0**.

Русский доступен для интерфейса и [документации](docs/ru/README.md). Настройки компиляции теперь предоставляют параметры VBSP, VVIS и VRAD выбранного внешнего компилятора. Статические пейзажи по умолчанию могут использовать модели с поддержкой физики; Для отсутствующего неба есть резервный вариант, а для черных карт освещения : отдельная диагностика.

[Инструмент дороги] (docs/ru/features/road-tool.md) добавляет локальную точку undo/redo, более широкие одиночные полосы движения и редактируемые предустановки T, четырехстороннего движения, Y и закругленного тупика. Боковые панели могут перемещаться независимо внутри редактора, вывод BSP может располагаться рядом с VMF, а сглаживание камеры имеет четкий переключатель. Предопределенные модели сущностей и тема запуска сохраняются.

[Создание браши для пропа] (docs/ru/features/propper.md) запрашивает статическое или подвижное физическое поведение, столкновение, массу и настройки LOD перед созданием модели.

[Предварительный просмотр ветра] (docs/ru/features/particles.md#wind-from-env_wind) показывает направление и порывы env_wind в 3D, с переносимой ветром пылью и дополнительной симуляцией веревки с учетом настроек ветра каждого объекта.

<a id="highlights"></a>
## Основные моменты

У каждой функции есть своя страница в [документации](docs/ru/README.md).

- **Встроенный редактор частиц.** Создавайте и редактируйте системы PCF в разделе **Другое > Редактор частиц** с типизированными параметрами, библиотеками операторов, отменой, предварительным просмотром в реальном времени, экспортом дополнений и элементами управления MCP. См. [Частицы](docs/ru/features/particles.md).

- **Мгновенная загрузка.** Браш из 100 000 VMF анализирует примерно за 0,3 с. Игровой контент (VPKs, дополнения, Мастерская) монтируется в фоновом режиме. См. [Настройка игры](docs/ru/getting-started/game-setup.md).
- **Обход VMF без потерь.** Карты, написанные Hammer, возвращаются без изменений, данные Hammer++ (блоки `vertices_plus`, `*_plus`) сохраняются. См. [Совместимость VMF](docs/ru/getting-started/vmf-compatibility.md).
- **Весь ваш контент.** Базовая игра VPKs, дополнения и файлы `.gma`, дополнения Workshop (также устаревшие `.bin`), игры `mount.cfg`, контент, загруженный с серверов, и [ваши собственные папки с текстурами] (docs/ru/features/custom-texture-folders.md), где обычный PNG, Изображения JPG, TGA и BMP работают как материалы.
- **Каждый инструмент Hammer.** [Выделение](docs/ru/features/selection-tool.md) с маркерами, 3D-манипуляторами растягивания и гизмо, [инструмент блока и примитивы](docs/ru/features/block-tool.md), [инструмент многоугольника](docs/ru/features/polygon-tool.md), [инструмент кривой](docs/ru/features/curve-tool.md), [дорога инструмент](docs/ru/features/road-tool.md), [детали дороги](docs/ru/features/road-details.md), [инструмент объекта](docs/ru/features/entity-tool.md), [обрезка](docs/ru/features/clip-tool.md), [инструмент среза](docs/ru/features/slice-tool.md), [инструмент вершин](docs/ru/features/vertex-tool.md), [дисплейсменты](docs/ru/features/displacement-tool.md), [текстура приложение](docs/ru/features/texture-tool.md), [декали и наложения](docs/ru/features/decals-overlays.md), [инструмент контура](docs/ru/features/path-tool.md), [разбрызгивание](docs/ru/features/sprinkle-tool.md), [кордоны](docs/ru/features/cordons.md), [группы](docs/ru/features/groups.md), [visgroups](docs/ru/features/visgroups.md), [экземпляры](docs/ru/features/instances.md), [префабы](docs/ru/features/prefabs.md), [редакторI/O](docs/ru/features/io-logical-view.md), [проверка на наличие проблем](docs/ru/features/check-problems.md).
- **Более Hammer.** [Инструмент для скоса кромки](docs/ru/features/bevel-tool.md) (скругление, фаска, выпуклость или дуга кромки с `Ctrl+B` в 3D-виде), [редактор UV](docs/ru/features/uv-editor.md), [редактор материалов](docs/ru/features/material-editor.md), узел на основе [редактор шейдеров](docs/ru/features/shader-editor.md) для изображений (шум, размытие, цветовые шкалы, карты нормалей, смешивание плиток), записанных как VTF и VMT, [VTF редактор](docs/ru/features/vtf-editor.md) (VTFEdit или встроенный), [пользовательский сетки](docs/ru/features/grids.md) (шаг по оси, повернут, прикреплен к объектам), [палитра команд](docs/ru/features/command-palette.md), [контур](docs/ru/features/outliner.md), [логическое представление](docs/ru/features/io-logical-view.md), [отмена выбора](docs/ru/features/undo-history.md), [автосохранение и сбой восстановление](docs/ru/features/autosave-crash.md), [несколько карт во вкладках](docs/ru/features/multiple-maps.md).
- **Смотрите карту как в игре.** [Предварительный просмотр освещения в реальном времени](docs/ru/features/lighting-preview.md) с тенями, картами нормалей и отражениями кубической карты, предварительный просмотр запечённого освещения и [вид игрока](docs/ru/features/player-view.md) (`F6`), который перемещается по скомпилированному миру со своими картами освещения.
- **Пропы и модели.** [Propper](docs/ru/features/propper.md) превращает браши в статические пропы (с LODs и столкновением) и пропы обратно в браши, [импорт модели](docs/ru/features/model-import.md) переносит FBX, OBJ и glTF в В игре [браузер моделей] (docs/ru/features/model-browser.md) просматривается каждая модель в 3D.
- **Собственный компилятор.** Геометрия, видимость и освещение (LDR и HDR, статическое освещение пропа) для Garry's Mod и других игр Source, с умными перекомпиляциями, фоновыми предварительными компиляциями и журналом компиляции с кликабельными ошибками. Вместо этого для каждой игры можно выбрать vbsp, vvis и vrad от Valve или собственные программы. См. [Встроенный компилятор](docs/ru/compiler/overview.md), [Выбор компилятора](docs/ru/compiler/compilers.md) и [Целевые игры](docs/ru/compiler/target-games.md).
- **Состояние карты.** Проверка проблем в реальном времени, мгновенная проверка утечек с указанием пути утечки, ограничений движка для каждой игры, файлов портала и видимости. См. [Утечки и файлы точек](docs/ru/features/leaks-pointfiles.md).
- **Отправьте.** [Ресурсы карты](docs/ru/features/map-assets.md) перечисляет и собирает все файлы, используемые картой, [Создание и сохранение как содержимое](docs/ru/features/build-content-release.md) создает готовое к совместному использованию дополнение с контрольным списком выпуска и публикацией в Мастерской, [Защитите свой BSP](docs/ru/features/bsp-protection.md) отправляет защищенную копию карты.
- **Помощники по игровому процессу.** [Генерация навигационной сетки](docs/ru/features/nav-mesh.md), [несколько трёхмерных скайбоксов и объект переключения скайбокса](docs/ru/features/sky-cameras.md), [звуковые ландшафты](docs/ru/features/soundscapes.md), [предварительный просмотр частиц](docs/ru/features/particles.md), [запуск и перезагрузка игры](docs/ru/features/game-launch.md).
- **AI помощники.** Встроенный [MCP сервер](docs/ru/reference/mcp-server.md) позволяет Клоду, Кодексу или Близнецам читать и редактировать открытую карту, проверять ее, компилировать и делать снимки экрана, причем каждое редактирование представляет собой один шаг отмены.
- **Ваш редактор.** Черно-белый интерфейс (темный или светлый), [английский, французский и русский](docs/ru/features/themes-languages.md), [наборы сочетаний клавиш Hammer и Hammer++](docs/ru/features/keyboard-shortcuts.md), [Discord Rich Presence](docs/ru/features/discord.md), [автоматические обновления](docs/ru/features/updates.md).

<a id="requirements"></a>
## Требования

- Windows 10 или 11 (64-разрядная версия). Сборки Linux также компилируются из исходного кода.
- A GPU с Vulkan, DirectX 12 или металлической опорой.
- Garry's Mod устанавливается через Steam (находится автоматически). Другие игры Source поддерживаются в качестве целей.

<a id="download-and-install"></a>
## Загрузите и установите

1. Загрузите `hammerpro.exe` со страницы «Релизы» этого репозитория или [соберите его из исходного кода] (docs/ru/development/building.md).
2. Поместите его в любую папку и запустите. Установщика нет.
3. При первом запуске помощник по настройке запросит ваш язык, редактор, из которого вы зашли (Hammer, Hammer++ или настройки по умолчанию Hammer Pro), а также ваш игровой контент.

Подробности: [Установка](docs/ru/getting-started/installation.md) и [Настройка игры](docs/ru/getting-started/game-setup.md).

<a id="quick-start"></a>
## Быстрый старт

1. Откройте `.vmf` (**Файл > Открыть карту...**, `Ctrl+O` или перетащите его в окно) или запустите новую карту (`Ctrl+N`).
2. Нарисуйте комнату с помощью инструмента «Блок» (`Shift+B`), разместите объекты с помощью инструмента «Entity» (`Shift+E`), текстурируйте грани с помощью `Shift+A`.
3. Нажмите `F9`, чтобы скомпилировать и запустить карту в Garry's Mod.

Следуйте [Ваша первая карта] (docs/ru/getting-started/first-map.md) для полного прохождения.

| Действие | Ключи |
|---|---|
| Управляйте 3D-камерой | удерживайте правую кнопку мыши + `W A S D` (`Z Q S D` на AZERTY), `Shift` быстрее |
| Переключить вид мыши | `Z` |
| Панорамирование/масштабирование 2D-изображений | средняя мышь или `Space` + перетаскивание/колесо мыши |
| Сетка меньше/крупнее | `[` / `]` |
| Инструменты «Выделение», «блок», «объект», «текстура», «обрезка», «вершины» | `Shift+S`, `Shift+B`, `Shift+E`, `Shift+A`, `Shift+X`, `Shift+V` |
| Палитра команд | `Ctrl+P` |
| Скомпилировать сейчас | `F9` |
| Просмотр игрока | `F6` |
| Каждый сочетание клавиш | `F1` |

Все сочетания клавиш перечислены в разделе [Действия и сочетания клавиш] (docs/ru/reference/actions-shortcuts.md) и их можно изменить в **Настройки > Клавиатура**.

<a id="building-from-source"></a>
## Сборка из исходников

```bash
git clone <this repository>
cd HammerPro
cargo build --release -p hammerpro
```

Исполняемый файл : `target/release/hammerpro.exe`. Требуется Rust 1.95 или новее. Запустите тесты с помощью `cargo test --workspace` (тесты, требующие наличия Garry's Mod, пропускаются, если он не установлен). См. [Сборка из исходного кода] (docs/ru/development/building.md) для получения профилей сборки и советов и [Архитектура] (docs/ru/development/architecture.md) для ящиков.

<a id="documentation"></a>
## Документация

- [Team Mapping](docs/ru/features/team-mapping.md): сеансы, приглашения, участники, безопасность и ограничения. [English](docs/en/features/team-mapping.md) | [Français](docs/fr/features/team-mapping.md).
- [Документация на русском](docs/ru/README.md).

- [Документация на английском языке](docs/en/README.md): начало работы, одна страница на каждую функцию, компилятор, ссылки (настройки, сочетания клавиш, командная строка, сценарии, MCP), руководства, устранение неполадок и разработка.
- [Документация на французском языке](docs/fr/README.md).
- [Справочник по инструменту MCP](docs/ru/MCP.md) (сгенерирован) и [руководство разработчика по расширению Hammer Pro](docs/EXTENDING.md).

<a id="community-and-contact"></a>
## Сообщество и контакты

- Сервер Discord: [discord.gg/eAEWWKRNAe](https://discord.gg/eAEWWKRNAe) (помощь, отзывы, отчеты об ошибках, новости)
- Автор в Discord: `oracleidt`

<a id="contributing"></a>
## Содействие

Взносы приветствуются. Пожалуйста, прочтите [CONTRIBUTING.md](CONTRIBUTING.md). Английский и французский переводы находятся рядом с кодом в таблице `TR` каждого ящика. Каталог русских интерфейсов входит в комплект поставки `crates/hp-app/src/i18n/ru.json`; документация доступна в `docs/en`, `docs/fr` и `docs/ru`.

<a id="license"></a>
## Лицензия

MIT. См. [LICENSE](LICENSE).

Hammer Pro не связан с Valve или Facepunch Studios. Source, Hammer и Garry's Mod являются торговыми марками соответствующих владельцев. Hammer Pro не включает и не распространяет какой-либо код Valve или игровой контент.
