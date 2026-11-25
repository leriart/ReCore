// ReCore's landing page: the latest release links for the visitor's system,
// and the live theme preview that mirrors Appearance's theme tiles.
(() => {
    "use strict";

    const $ = id => document.getElementById(id);
    const tr = text => (window.t ? window.t(text) : text);
    const RELEASES = "https://api.github.com/repos/leriart/ReCore/releases?per_page=8";
    const ALL_RELEASES = "https://github.com/leriart/ReCore/releases";
    const LATEST_RELEASE = `${ALL_RELEASES}/latest`;
    const LATEST_CACHE = "recore-latest-release";
    // Shown at once, before GitHub answers, and kept if the API is rate
    // limited or offline. The API replaces it as soon as it loads; bump this
    // block with scripts/set-version when a new stable release ships.
    const FALLBACK_RELEASE = {
        tag_name: "v0.5.7",
        name: "ReCore 0.5.7",
        html_url: "https://github.com/leriart/ReCore/releases/tag/v0.5.7",
        published_at: "2026-11-25T05:44:02Z",
        assets: [
            {
                name: "ReCore-0.5.7-windows-x64.zip",
                browser_download_url: "https://github.com/leriart/ReCore/releases/download/v0.5.7/ReCore-0.5.7-windows-x64.zip"
            },
            {
                name: "ReCore-0.5.7-x86_64.AppImage",
                browser_download_url: "https://github.com/leriart/ReCore/releases/download/v0.5.7/ReCore-0.5.7-x86_64.AppImage"
            },
            {
                name: "recore-0.5.7-1-x86_64.pkg.tar.zst",
                browser_download_url: "https://github.com/leriart/ReCore/releases/download/v0.5.7/recore-0.5.7-1-x86_64.pkg.tar.zst"
            },
            {
                name: "ReCore-0.5.7-macos-arm64.dmg",
                browser_download_url: "https://github.com/leriart/ReCore/releases/download/v0.5.7/ReCore-0.5.7-macos-arm64.dmg"
            },
            {
                name: "ReCore-0.5.7-macos-arm64.zip",
                browser_download_url: "https://github.com/leriart/ReCore/releases/download/v0.5.7/ReCore-0.5.7-macos-arm64.zip"
            },
            {
                name: "ReCore-0.5.7-macos-x64.dmg",
                browser_download_url: "https://github.com/leriart/ReCore/releases/download/v0.5.7/ReCore-0.5.7-macos-x64.dmg"
            },
            {
                name: "ReCore-0.5.7-macos-x64.zip",
                browser_download_url: "https://github.com/leriart/ReCore/releases/download/v0.5.7/ReCore-0.5.7-macos-x64.zip"
            },
            {
                name: "ReCore-0.5.7-source.zip",
                browser_download_url: "https://github.com/leriart/ReCore/releases/download/v0.5.7/ReCore-0.5.7-source.zip"
            },
            {
                name: "SHA256SUMS.txt",
                browser_download_url: "https://github.com/leriart/ReCore/releases/download/v0.5.7/SHA256SUMS.txt"
            }
        ]
    };

    function detectPlatform() {
        const hint = `${navigator.userAgentData?.platform ?? ""} ${navigator.platform ?? ""} ${navigator.userAgent}`.toLowerCase();
        if (/android|iphone|ipad|ipod/.test(hint)) return { os: "mobile", mobile: true };
        if (/win/.test(hint)) return { os: "windows" };
        if (/mac|darwin/.test(hint)) return { os: "mac" };
        return { os: "linux" };
    }

    const platform = detectPlatform();

    function assets(release) {
        return release?.assets ?? [];
    }

    function findAsset(release, pattern) {
        return assets(release).find(asset => pattern.test(asset.name));
    }

    function platformAsset(release) {
        if (platform.os === "windows") {
            return findAsset(release, /windows-x64\.zip$/i) ?? findAsset(release, /windows.*setup.*\.exe$/i);
        }
        if (platform.os === "linux") return findAsset(release, /\.AppImage$/i);
        if (platform.os === "mac") return findAsset(release, /macos-(?:arm64|x64)\.(?:dmg|zip)$/i);
        return null;
    }

    function size(bytes) {
        if (!Number.isFinite(bytes)) return "";
        if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
        if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KB`;
        return `${bytes} B`;
    }

    function assetLabel(name) {
        if (name.endsWith(".AppImage.zsync")) return tr("AppImage update");
        if (name.endsWith(".dmg")) return tr("macOS DMG");
        if (/macos-(?:arm64|x64)\.zip$/i.test(name)) return tr("macOS ZIP");
        if (name.endsWith(".AppImage")) return tr("AppImage");
        if (name.includes(".pkg.tar.zst")) return tr("Arch package");
        if (/windows.*x64.*\.zip$/i.test(name)) return tr("Windows ZIP");
        if (/windows.*setup.*\.exe$/i.test(name)) return tr("Windows installer");
        if (/source\.zip$/i.test(name)) return tr("Source");
        if (name.endsWith("SHA256SUMS.txt")) return tr("Checksums");
        if (name.endsWith(".minisig")) return tr("Signature");
        return name;
    }

    function setLink(id, asset, fallback, label, detail) {
        const link = $(id);
        if (!link) return;
        const text = link.querySelector("span");
        const note = link.querySelector("small");
        if (asset) {
            link.href = asset.browser_download_url;
            link.download = asset.name;
            link.classList.remove("is-missing");
            if (text) text.textContent = label;
            if (note) note.textContent = detail ?? size(asset.size);
        } else {
            link.href = fallback;
            link.classList.add("is-missing");
            if (text) text.textContent = tr("See releases");
            if (note) note.textContent = "";
        }
    }

    function releaseDate(release) {
        if (!release.published_at) return "";
        try {
            return new Intl.DateTimeFormat(document.documentElement.lang || "en", {
                dateStyle: "medium"
            }).format(new Date(release.published_at));
        } catch {
            return "";
        }
    }

    function pickLatest(releases) {
        return releases.find(release => !release.prerelease) ?? releases[0] ?? null;
    }

    function setPrimary(release) {
        const asset = platformAsset(release);
        const system = platform.os === "windows" ? "WINDOWS" : platform.os === "linux" ? "LINUX" : "MACOS";
        const heroLabel = platform.mobile
            ? tr("DESKTOP ONLY")
            : asset
                ? tr(`FOR ${system}`)
                : tr("LATEST RELEASE");
        const installLabel = platform.mobile
            ? tr("DESKTOP ONLY")
            : asset
                ? platform.os === "windows"
                    ? tr("WINDOWS ZIP")
                    : platform.os === "linux"
                        ? tr("LINUX APPIMAGE")
                        : tr("MACOS APP")
                : tr("LATEST RELEASE");
        for (const [linkId, labelId, label] of [
            ["download-primary", "download-primary-label", heroLabel],
            ["install-primary", "install-primary-label", installLabel]
        ]) {
            const link = $(linkId);
            const target = $(labelId);
            if (!link || !target) continue;
            target.textContent = label;
            if (asset) {
                link.href = asset.browser_download_url;
                link.download = asset.name;
            } else {
                link.href = LATEST_RELEASE;
                link.removeAttribute("download");
            }
        }
    }

    function setInstallLinks(latest) {
        if (!latest) return;
        const fallback = latest.html_url || LATEST_RELEASE;
        const date = releaseDate(latest);
        if ($("top-version")) $("top-version").textContent = latest.tag_name.toUpperCase();
        if ($("release-version")) $("release-version").textContent = latest.tag_name;
        if ($("install-version")) $("install-version").textContent = `${latest.tag_name}${date ? ` · ${date}` : ""}`;

        setLink("download-windows", findAsset(latest, /windows-x64\.zip$/i), fallback, tr("Windows ZIP"), null);
        setLink("download-linux", findAsset(latest, /\.AppImage$/i), fallback, tr("AppImage"), null);
        setLink("download-arch", findAsset(latest, /\.pkg\.tar\.zst$/i), fallback, tr("Arch package"), null);
        setLink("download-mac", findAsset(latest, /\.dmg$/i), fallback, tr("macOS DMG"), null);
        setLink("download-mac-zip", findAsset(latest, /macos-(?:arm64|x64)\.zip$/i), fallback, tr("macOS ZIP"), null);
        const source = findAsset(latest, /source\.zip$/i);
        const sourceLink = $("download-source");
        if (sourceLink) {
            sourceLink.href = source?.browser_download_url ?? fallback;
            if (source) sourceLink.download = source.name;
        }

        document.querySelectorAll(".os-download").forEach(card => {
            card.classList.toggle("is-detected", card.dataset.os === platform.os);
        });
        const note = $("install-note");
        if (note) {
            note.textContent = platform.mobile
                ? tr("ReCore is a desktop client. Open this page on the computer where you use Discord.")
                : tr("Downloads point to the latest stable release on GitHub. The checksums are published beside them.");
        }
        const hint = $("install-os-hint");
        if (hint) {
            hint.textContent = platform.mobile
                ? tr("DESKTOP APP")
                : `${tr("RECOMMENDED")}: ${platform.os === "windows" ? "WINDOWS" : platform.os === "mac" ? "MACOS" : "LINUX"}`;
        }
        setPrimary(latest);
    }

    function releaseCard(release, latestTag) {
        const article = document.createElement("article");
        article.className = "release-card";
        const head = document.createElement("div");
        head.className = "release-head";
        const title = document.createElement("a");
        title.className = "release-title";
        title.href = release.html_url;
        title.textContent = release.name || release.tag_name;
        head.append(title);
        if (release.tag_name === latestTag) {
            const badge = document.createElement("span");
            badge.className = "release-badge";
            badge.textContent = tr("LATEST");
            head.append(badge);
        }
        if (release.prerelease) {
            const badge = document.createElement("span");
            badge.className = "release-badge release-badge--pre";
            badge.textContent = tr("PRE-RELEASE");
            head.append(badge);
        }
        article.append(head);

        const meta = document.createElement("p");
        meta.className = "release-meta";
        const date = releaseDate(release);
        meta.textContent = `${release.tag_name}${date ? ` · ${date}` : ""}`;
        article.append(meta);

        if (assets(release).length) {
            const grid = document.createElement("div");
            grid.className = "release-assets";
            for (const asset of assets(release)) {
                const link = document.createElement("a");
                link.className = "release-asset";
                link.href = asset.browser_download_url;
                link.download = asset.name;
                const label = document.createElement("span");
                label.textContent = assetLabel(asset.name);
                const detail = document.createElement("small");
                detail.textContent = `${size(asset.size)}${asset.download_count ? ` · ${asset.download_count} ${tr("downloads")}` : ""}`;
                link.append(label, detail);
                grid.append(link);
            }
            article.append(grid);
        }
        return article;
    }

    function showReleaseError(error) {
        console.warn("ReCore releases:", error);
        const status = $("release-status");
        const heroStatus = $("hero-release-status");
        if (status) status.textContent = tr("Could not check GitHub. Use the release page for the latest files.");
        if (heroStatus) heroStatus.textContent = tr("Could not check GitHub.");
        const note = $("install-note");
        if (note) note.textContent = tr("Could not check GitHub. Showing the last known release links.");
        for (const id of ["download-windows", "download-linux", "download-arch", "download-mac", "download-mac-zip", "download-source"]) {
            const link = $(id);
            if (link && !link.href.includes("/releases/download/")) link.href = LATEST_RELEASE;
        }
        const list = $("release-list");
        if (list) {
            const message = document.createElement("p");
            message.className = "section-note";
            message.append(tr("Could not load the releases. "));
            const link = document.createElement("a");
            link.href = ALL_RELEASES;
            link.textContent = tr("Open releases on GitHub");
            message.append(link);
            if (list.querySelector(".release-card")) list.append(message);
            else list.replaceChildren(message);
        }
    }

    async function loadReleases() {
        let cached = FALLBACK_RELEASE;
        try {
            const stored = localStorage.getItem(LATEST_CACHE);
            if (stored) cached = JSON.parse(stored);
        } catch {}
        setInstallLinks(cached);
        const initialList = $("release-list");
        if (initialList) initialList.replaceChildren(releaseCard(cached, cached.tag_name));
        try {
            const controller = new AbortController();
            const timeout = setTimeout(() => controller.abort(), 8000);
            let response;
            try {
                response = await fetch(RELEASES, {
                    headers: { Accept: "application/vnd.github+json" },
                    signal: controller.signal
                });
            } finally {
                clearTimeout(timeout);
            }
            if (!response.ok) throw new Error(`GitHub returned ${response.status}`);
            const releases = await response.json();
            if (!Array.isArray(releases)) throw new Error("GitHub returned no release list");
            const latest = pickLatest(releases);
            if (!latest) {
                showReleaseError(new Error("no releases"));
                return;
            }
            setInstallLinks(latest);
            try { localStorage.setItem(LATEST_CACHE, JSON.stringify(latest)); } catch {}
            const status = $("release-status");
            const heroStatus = $("hero-release-status");
            const date = releaseDate(latest);
            if (status) status.textContent = `${latest.tag_name}${date ? ` · ${date}` : ""}`;
            if (heroStatus) heroStatus.textContent = date;
            const list = $("release-list");
            if (list) {
                list.replaceChildren();
                for (const release of releases.slice(0, 6)) {
                    list.append(releaseCard(release, latest.tag_name));
                }
            }
        } catch (error) {
            showReleaseError(error);
        }
    }

    function buildPreview() {
        const markup = `
            <i class="rc-chat"></i>
            <i class="rc-rail"></i>
            <i class="rc-sidebar"></i>
            <i class="rc-server rc-server--active"></i>
            <i class="rc-server rc-server--two"></i>
            <i class="rc-server rc-server--three"></i>
            <i class="rc-channel rc-channel--one"></i>
            <i class="rc-channel rc-channel--two"></i>
            <i class="rc-channel rc-channel--three"></i>
            <i class="rc-channel rc-channel--four"></i>
            <span class="rc-message rc-message--first"><i class="rc-avatar"></i><i class="rc-name"></i><i class="rc-line"></i></span>
            <span class="rc-message rc-message--second"><i class="rc-avatar"></i><i class="rc-name"></i><i class="rc-line"></i></span>
            <span class="rc-composer"><i></i></span>`;
        for (const preview of document.querySelectorAll(".rc-preview")) {
            if (!preview.firstElementChild) preview.innerHTML = markup;
        }
    }

    const themes = {
        recore: {
            label: "ReCore",
            note: "Signal orange on warm charcoal.",
            accent: "#e94221"
        },
        paper: {
            label: "ReCore Paper",
            note: "The same signal on warm paper.",
            accent: "#e94221"
        },
        ambxst: {
            label: "Ambxst",
            note: "Material You rose on charcoal.",
            accent: "#ffb3ae"
        },
        "ambxst-paper": {
            label: "Ambxst Paper",
            note: "The Ambxst palette in daylight.",
            accent: "#904a46"
        },
        catppuccin: {
            label: "Catppuccin Mocha",
            note: "Mauve on a midnight base.",
            accent: "#cba6f7"
        },
        "tokyo-night": {
            label: "Tokyo Night",
            note: "Ink blue with neon signals.",
            accent: "#7aa2f7"
        },
        nord: {
            label: "Nord Aurora",
            note: "Arctic grounds and aurora green.",
            accent: "#a3be8c"
        },
        gruvbox: {
            label: "Gruvbox Ember",
            note: "Warm retro browns and ember orange.",
            accent: "#fe8019"
        },
        solarized: {
            label: "Solarized Paper",
            note: "Solarized's light paper palette.",
            accent: "#2aa198"
        }
    };

    function initPreview() {
        buildPreview();
        const stage = $("theme-stage");
        const accent = $("preview-accent");
        const accentValue = $("preview-accent-value");
        const name = $("theme-name");
        const note = $("theme-note");
        const choices = [...document.querySelectorAll(".theme-choice")];
        if (!stage || !choices.length) return;

        function useTheme(id) {
            const theme = themes[id];
            if (!theme) return;
            stage.dataset.theme = id;
            stage.style.removeProperty("--p-accent");
            if (name) name.textContent = tr(theme.label);
            if (note) note.textContent = tr(theme.note);
            if (accent) accent.value = theme.accent;
            if (accentValue) accentValue.textContent = theme.accent;
            for (const choice of choices) {
                const active = choice.dataset.theme === id;
                choice.classList.toggle("is-active", active);
                choice.setAttribute("aria-pressed", String(active));
            }
        }

        for (const choice of choices) {
            choice.addEventListener("click", () => useTheme(choice.dataset.theme));
        }

        if (accent) {
            accent.addEventListener("input", () => {
                stage.style.setProperty("--p-accent", accent.value);
                if (accentValue) accentValue.textContent = accent.value;
            });
        }

        const builder = $("accent");
        if (builder) {
            builder.addEventListener("input", () => {
                if (!accent) return;
                accent.value = builder.value;
                if (accentValue) accentValue.textContent = builder.value;
                stage.style.setProperty("--p-accent", builder.value);
            });
        }

        useTheme(stage.dataset.theme || "recore");
    }

    function init() {
        initPreview();
        void loadReleases();
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", init, { once: true });
    } else {
        init();
    }
})();
