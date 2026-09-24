/**
 * Разделы ИИ, Голос и Настройки.
 *
 * Раньше всё это было одной страницей настроек — двадцать блоков подряд, от
 * ключа модели до переноса данных. Разложено по тому, как об этом думает
 * человек: чем Yuki думает и что помнит (ИИ), как её зовут и как она говорит
 * (Голос), и всё остальное (Настройки). Навигация повторяет Astra.
 */

import { SectionPage } from '../design-system/components/SectionPage'
import { useUiStore } from '../state/store'
import { Activity } from './Activity'
import { Memory } from './Memory'
import { Notes } from './Notes'
import {
  Appearance, Background, Calendar, CapabilitiesEntry, Character, DataTransfer, Everyday, Hotkey,
  LocalSpeech, PersonaSection, Permissions, Privacy, Providers, Remote, Speech, SystemSection,
  Updates, Voice, WakeWord,
} from './Settings'
import { VoiceInstaller } from './VoiceInstaller'
import { VoiceSetup } from './VoiceSetup'
import './Settings.css'

export function AiPage() {
  return <SectionPage page="ai" sections={[
    { id: 'model', label: 'Модель', render: () => <Providers /> },
    { id: 'persona', label: 'Характер', render: () => <><Character /><PersonaSection /></> },
    { id: 'memory', label: 'Память', render: () => <div className="section-page__embed"><Memory /></div> },
    { id: 'notes', label: 'Заметки', render: () => <div className="section-page__embed"><Notes /></div> },
  ]} />
}

export function VoicePage() {
  return <SectionPage page="voice" sections={[
    { id: 'wake', label: 'Как позвать', render: () => <><WakeWord /><Voice /><Hotkey /></> },
    { id: 'stt', label: 'Распознавание', render: () => <><LocalSpeech /><VoiceSetup /></> },
    { id: 'tts', label: 'Голос Yuki', render: () => <><VoiceInstaller /><Speech /></> },
  ]} />
}

export function SettingsPage({ initial }: { initial?: string }) {
  const setScreen = useUiStore((s) => s.setScreen)
  return <SectionPage page="settings" initial={initial} sections={[
    { id: 'connections', label: 'Подключения', render: () => <><Remote /><Calendar /><Everyday /><CapabilitiesEntry /></> },
    { id: 'permissions', label: 'Разрешения', render: () => <Permissions /> },
    { id: 'privacy', label: 'Приватность', render: () => <Privacy /> },
    { id: 'app', label: 'Приложение', render: () => <><Appearance /><Background /><Updates /><SystemSection /></> },
    { id: 'journal', label: 'Журнал действий', render: () => <div className="section-page__embed"><Activity /></div> },
    { id: 'data', label: 'Данные и перенос', render: () => <DataTransfer /> },
  ]} footer={<>Облик, движения и голос персонажа — в разделе <button type="button" className="section-page__link" onClick={() => setScreen('companion')}>Персонаж</button></>} />
}
