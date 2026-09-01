import i18n from "i18next";
import { initReactI18next } from "react-i18next";

const resources = {
  en: {
    translation: {
      brand: "Furfolio",
      navigation: {
        library: "Library",
        settings: "Settings",
      },
      empty: {
        eyebrow: "Your private character library",
        title: "Keep every character detail in one safe place.",
        description:
          "Furfolio stores references, credits, colors, and notes on this device. No account and no cloud upload.",
        create: "Create first character",
        restore: "Restore backup",
      },
      privacy: {
        title: "Local by design",
        description: "Your library stays on this device unless you export it.",
      },
      language: {
        label: "Language",
        english: "English",
        chinese: "简体中文",
      },
      theme: {
        label: "Theme",
        light: "Use light theme",
        dark: "Use dark theme",
      },
      status: "Furfolio v0.1 foundation",
    },
  },
  "zh-CN": {
    translation: {
      brand: "Furfolio",
      navigation: {
        library: "角色库",
        settings: "设置",
      },
      empty: {
        eyebrow: "你的私人兽设资料库",
        title: "把每一份角色设定，安心整理在一处。",
        description: "Furfolio 在本机保存参考图、作者信息、配色与备注，无需账号，也不会上传到云端。",
        create: "创建第一个角色",
        restore: "恢复备份",
      },
      privacy: {
        title: "为本地保存而设计",
        description: "除非你主动导出，否则资料库只留在这台设备上。",
      },
      language: {
        label: "语言",
        english: "English",
        chinese: "简体中文",
      },
      theme: {
        label: "主题",
        light: "切换为浅色主题",
        dark: "切换为深色主题",
      },
      status: "Furfolio v0.1 工程基础",
    },
  },
} as const;

const savedLanguage = localStorage.getItem("furfolio-language");
const systemLanguage = navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";

void i18n.use(initReactI18next).init({
  resources,
  lng: savedLanguage === "en" || savedLanguage === "zh-CN" ? savedLanguage : systemLanguage,
  fallbackLng: "en",
  interpolation: {
    escapeValue: false,
  },
});

export default i18n;
