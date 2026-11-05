# Bose Connect para Linux: español.
#
# Traduce los mensajes de en.ftl (el idioma de origen); ver
# CONTRIBUTING.md, "Translating the GUI".

## Ventana, barra de título y menú principal

page-overview-subtitle = Panel de control y ajustes
page-mybose-title = Mis dispositivos Bose
page-mybose-subtitle = Dispositivos Bose emparejados con esta computadora
header-back = Volver al dispositivo
header-theme = Cambiar tema claro / oscuro
header-menu = Menú principal
menu-refresh = Actualizar información
menu-mybose = Mis dispositivos Bose
menu-about = Acerca de Bose Connect
menu-quit = Salir
about-comments =
    Control de auriculares y altavoces Bose sobre RFCOMM Bluetooth en Linux.
    Motor de protocolo: bose-connect v{ $version }

## Menú de dispositivos (arriba a la izquierda)

device-button-connecting = Conectando…
device-button-none = Sin dispositivo
device-menu-title = Dispositivos Bose emparejados
device-menu-empty = Ningún dispositivo Bose emparejado con esta computadora
device-menu-connected = { $address } · Conectado
device-menu-disconnected = { $address } · Sin conexión
device-menu-manage = Gestionar dispositivos…
bluez-down-title = BlueZ no responde
bluez-down-subtitle = No se pudo leer la lista de dispositivos emparejados

## Diálogos

dialog-cancel = Cancelar
dialog-save = Guardar
rename-title = Renombrar dispositivo
rename-body = Introduce el nuevo nombre Bluetooth del dispositivo (máximo 31 bytes):
rename-counter = { $length } / 31 bytes
forget-title = ¿Olvidar dispositivo?
forget-body = ¿Eliminar “{ $name }” ({ $address }) de la memoria del dispositivo Bose? Tendrás que volver a emparejarlo a mano.
forget-confirm = Olvidar

## Tarjeta del dispositivo

hero-connecting = Conectando a { $name }…
hero-failed = No se pudo conectar
hero-none = Ningún dispositivo conectado
hero-pick = Elige un dispositivo para controlarlo
hero-rename = Renombrar dispositivo
hero-via-bd-addr = Conectado vía RFCOMM · BD_ADDR: { $address }
hero-via-mac = Conectado vía RFCOMM · MAC: { $address }
hero-history = últimas 32 lecturas
status-connected = ● Conectado
status-connecting = ○ Conectando…
status-disconnected = ○ Desconectado

## Audio y reproducción

group-audio = Audio y reproducción
audio-row-title = Audio en esta computadora
audio-unknown = No emparejado con esta computadora
audio-disconnected = Sin conexión de audio con esta computadora
audio-without-audio = Conectado, pero sin audio funcional desde esta computadora
audio-idle = Audio listo (en pausa)
audio-playing = Reproduciendo audio de esta computadora
audio-reconnect-hint = Conectado, pero sin audio funcional desde esta computadora. Si esperas sonido de aquí, desconecta el dispositivo y vuelve a conectarlo desde esta computadora: cuando el dispositivo inicia la conexión, BlueZ puede no abrir el audio.
media-title = Reproducción
media-subtitle = Pausa, reanuda o cambia de pista
media-previous = Pista anterior
media-play = Reproducir
media-pause = Pausar
media-next = Pista siguiente
volume-title = Volumen del dispositivo
volume-subtitle = Nivel de volumen de Bose (0 a 75)
volume-value = { $level } / 75 ({ $percent }%)

## Control del auricular

group-headset = Control del auricular
anc-title = Cancelación activa de ruido (ANC)
anc-subtitle = Cuánto se atenúa el sonido exterior
level-off = Desactivado
level-low = Bajo
level-medium = Medio
level-high = Alto
self-voice-title = Voz propia
self-voice-subtitle = Escucha tu propia voz con naturalidad durante llamadas

## Indicaciones de voz

group-voice = Voz e indicaciones
voice-prompts-title = Indicaciones de voz
voice-prompts-subtitle = Anuncios hablados de estado y batería
prompt-language-title = Idioma de las indicaciones
prompt-language-subtitle = Idioma de las indicaciones de voz
prompt-language-unknown = Desconocido ({ $code })
prompt-language-read-only = Cámbialo en la app de Bose del teléfono: esta computadora aún no puede cambiarlo en este dispositivo

## Energía

group-power = Energía y batería
auto-off-title = Apagado automático
auto-off-subtitle = Apaga el dispositivo tras un tiempo sin uso para ahorrar batería
auto-off-never = Nunca
auto-off-minutes = { $minutes } min

## Fuentes emparejadas con el dispositivo Bose (multipunto)

group-multipoint = Fuentes emparejadas con el Bose (multipunto)
pairing-title = Visible para emparejar
pairing-subtitle = Permite a otros teléfonos o computadoras descubrir este Bose
sources-empty = No hay otros dispositivos emparejados
sources-empty-hint = Activa “Visible para emparejar” para conectar equipos nuevos
source-this = Esta computadora
source-active-audio = Audio activo
source-unnamed = Dispositivo { $address }
action-connect = Conectar
action-disconnect = Desconectar
action-forget = Olvidar dispositivo

## Información del dispositivo

group-info = Información del dispositivo
info-device-id = Identificador de hardware
info-bd-addr = Dirección Bluetooth física (BD_ADDR)
info-bd-addr-subtitle = Dirección MAC de hardware del módulo Bose
info-firmware = Versión de firmware
info-serial = Número de serie
info-serial-show = Mostrar número de serie
info-serial-hide = Ocultar número de serie
group-advanced = Avanzado
info-versions = Versiones

## Sin conexión, sin dispositivos emparejados, página Mis dispositivos

pairing-steps =
    1. Pon el dispositivo Bose en modo de emparejamiento: mantén pulsado su botón de Bluetooth hasta que la luz parpadee en azul.
    2. Emparéjalo desde la configuración de Bluetooth del escritorio (o con bluetoothctl).
    3. Aparecerá aquí y en el menú de dispositivos.
no-devices-title = Ningún dispositivo Bose emparejado
no-devices-body = Esta computadora no tiene ningún dispositivo Bose emparejado.
no-devices-again = Buscar de nuevo
device-connected = Conectado
device-not-connected = Sin conexión
device-in-use = En uso
action-use = Usar
action-connecting = Conectando…
connect-group-title = Conéctate a un dispositivo
connect-group-description = Elige uno de los dispositivos Bose emparejados con esta computadora.
mybose-group-title = Dispositivos Bose emparejados con esta computadora
mybose-reload = Leer de nuevo la lista de BlueZ
mybose-pair-title = Emparejar un dispositivo nuevo

## Notificaciones del escritorio e icono de la bandeja

notify-battery-critical = Batería crítica: último 5 %. Conecta el cargador ya.
notify-battery-low = Batería baja: conecta el cargador pronto.
notify-battery-threshold = La batería bajó del umbral.
tray-battery = Bose Connect ({ $percent }%)
tray-disconnected = { $name }: desconectado
tray-battery-low = Bose Connect: batería baja

# Party mode (SoundLink Color II)
group-party = Modo fiesta
party-description = Dos SoundLink Color II tocan la misma música: esta bocina la recibe y la reenvía a la otra.
party-title = Segunda bocina
party-other-speaker = la otra bocina
party-mode-unknown = Desconocido: la bocina no respondió
party-mode-off = Sin unir a otra bocina
party-mode-party = Fiesta: esta bocina y { $peer } tocan la misma música
party-mode-stereo = Estéreo: esta bocina toca el canal izquierdo y { $peer } el derecho
party-choice-party = Fiesta
party-choice-stereo = Estéreo
party-join = Unir con…
party-leave = Separar
party-leave-unknown = Une la otra bocina desde esta aplicación para poder separarla aquí
party-busy = Esperando a la bocina…
party-error = La bocina no lo hizo
party-lockup-title = Estas bocinas se bloquean a veces
party-lockup-body = En modo fiesta una SoundLink Color II a veces deja de responder, también con un teléfono y la aplicación de Bose. Apágala y vuelve a encenderla.
party-rate-title = Enlace de audio desde este equipo
party-rate-ok = SBC a { $rate } kHz, como lo necesita el modo fiesta
party-rate-wrong = SBC a { $rate } kHz: con esta frecuencia el modo fiesta bloqueó las bocinas; necesita 44.1 kHz. Apaga y enciende la bocina y deja que se conecte sola, o haz que este equipo use 44.1 kHz.
party-rate-reconnect = SBC a { $rate } kHz: este equipo ya usa 44.1 kHz para los enlaces nuevos. Desconecta la bocina y vuelve a conectarla.
party-rate-fix = Usar 44.1 kHz
party-rate-fix-title = ¿Usar 44.1 kHz para el audio Bluetooth?
party-rate-fix-body = Esto escribe un ajuste de WirePlumber para que los enlaces de audio Bluetooth que abre este equipo usen 44.1 kHz en lugar de 48 kHz, para todos los dispositivos Bluetooth, y reinicia WirePlumber: el sonido se detiene un momento.
party-join-title = Unir otra bocina
party-join-body = Las dos bocinas deben estar encendidas. Elige la otra bocina (no un teléfono ni un equipo: la bocina intenta conectarse a él como bocina) y el dispositivo que toca la música.
party-join-no-peer = Primero vincula la otra SoundLink Color II con este equipo, para conocer aquí su dirección.
party-join-no-source = No hay ninguna fuente de música conectada a esta bocina. Conecta primero el audio de este equipo o un teléfono.
party-join-peer = Otra bocina
party-join-source = Música desde
party-join-confirm = Unir
party-source-this = Este equipo
party-wait-title = Esperando un minuto de silencio
party-wait-checking = Revisando el audio de este equipo…
party-wait-counting = Uniendo en { $seconds } s.
party-wait-playing = Este equipo está enviando audio a la bocina: páusalo. La cuenta regresiva vuelve a empezar cuando se detenga.
party-wait-no-link = Este equipo no tiene enlace de audio con la bocina. Vuelve a conectarla, o cancela y elige otra fuente.
party-wait-join-now = Unir ya
party-wait-hint = Las bocinas se bloquearon al unirlas mientras este equipo enviaba audio, o menos de un minuto después de que se detuvo. "Unir ya" se salta la espera con ese riesgo.
