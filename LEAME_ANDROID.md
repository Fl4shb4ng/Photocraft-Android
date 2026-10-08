# PhotoCraft para Android

Adaptación nativa experimental de PhotoCraft 0.3.0, basada en el commit
`727daf5d6745ba084d1feef02014779354ab4893` de `storytold/photocraft`.
La edición conserva el motor Rust y egui; una pequeña actividad Java conecta
el selector de archivos de Android. No se utiliza una webview.

## Instalar y usar

El APK entregado es una compilación de desarrollo optimizada, firmada con un
certificado de prueba, para teléfonos **ARM64 con Android 8 o posterior**.
Abra el APK en su teléfono y permita la instalación desde la aplicación que
utiliza para abrirlo. No es una publicación en Google Play.

**La compilación disponible todavía no incluye la corrección de pantalla
completa.** La captura de prueba mostró la barra de estado; ya corregí el código,
pero hace falta generar otro APK para que el cambio llegue al teléfono.

- **Open** abre imágenes y documentos con el selector del sistema.
- **New** crea un lienzo. La barra inferior contiene herramientas y paneles.
- Un dedo utiliza la herramienta; dos dedos desplazan y amplían el lienzo.
- **Layers** permite seleccionar capas y controlar visibilidad y opacidad.
- **Save** permite guardar en PSD, PCraft, PNG o JPEG. PSD y PCraft conservan capas.
- Los paneles se sitúan abajo en vertical y al costado en horizontal.
- En el código actualizado, las barras del sistema se ocultan al entrar y
  reaparecen temporalmente con un gesto desde el borde; al volver del selector
  de archivos, la pantalla completa se restablece.

Cancelar el selector o encontrar un error de escritura conserva el documento
abierto y con cambios pendientes. Las preferencias y los puntos de recuperación
se guardan en el espacio privado de la aplicación.

## Compilar

Necesita Linux/macOS o WSL2, Rust 1.95+, JDK 17, Python 3 y el SDK de Android.
Desde la raíz del código fuente:

```sh
rustup target add aarch64-linux-android
sdkmanager 'platforms;android-35' 'build-tools;35.0.0' 'ndk;28.2.13676358'
export ANDROID_SDK_ROOT=/ruta/al/android-sdk
packaging/android/build.sh
```

El resultado queda en `dist/android/PhotoCraft-arm64-v8a.apk`.
El script compila Rust y Java, empaqueta, alinea y firma el APK. Por defecto
compila en modo release. Para reproducir el perfil del APK entregado:

```sh
PHOTOCRAFT_ANDROID_PROFILE=debug packaging/android/build.sh
```

También se incluye `.github/workflows/android.yml` para generar el APK con
GitHub Actions. La configuración de firma propia y del emulador x86_64 se
explica en `docs/android/README.md`.

## Alcance de esta versión

La captura de prueba en un Motorola Edge 40 Pro con Android 16 confirmó que el
APK inicia. También mostró la barra de estado visible; se agregó un ajuste de
pantalla inmersiva al código fuente. Esa corrección todavía necesita compilarse
y probarse en el teléfono. Falta comprobar el teclado, los drivers gráficos,
los cambios de ciclo de vida, el selector de archivos y el rendimiento con
documentos grandes. Las capturas de diseño incluidas se generaron con el
renderizador de la interfaz compartida, fuera de Android.

Algunos flujos de escritorio, como colocar objetos vinculados e importar
presets mediante un selector síncrono, siguen sin integración móvil. El
portapapeles de imágenes del sistema y la accesibilidad de lector de pantalla
tampoco están implementados en este backend.

Detalles y resultados: `docs/android/README.md` y `docs/android/VALIDATION.md`.
El repositorio remoto original no fue modificado.
