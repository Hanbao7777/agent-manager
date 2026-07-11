import "@testing-library/jest-dom";
import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import "./msw/tauriMocks";

void i18n.use(initReactI18next).init({
  lng: "zh",
  fallbackLng: "zh",
  resources: {
    zh: { translation: {} },
    en: { translation: {} },
  },
  interpolation: {
    escapeValue: false,
  },
});

afterEach(() => {
  cleanup();
});
