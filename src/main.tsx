import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { InternationalizationProvider } from '@astryxdesign/core/i18n';
import zhCN from '@astryxdesign/core/locales/zh-CN.generated.js';
import { App } from './App';
import './styles.css';
createRoot(document.getElementById('root')!).render(<StrictMode><InternationalizationProvider locale="zh-CN" messages={{ "zh-CN": zhCN }}><App /></InternationalizationProvider></StrictMode>);
