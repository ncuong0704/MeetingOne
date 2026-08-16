'use client';

import React, { useState, useMemo, useEffect, useCallback } from 'react';
import { ChevronDown, ChevronRight, Settings, ChevronsLeft, ChevronsRight, Calendar, Home, Trash2, Mic, Square, Plus, Pencil, NotebookPen, SearchIcon, X, Upload } from 'lucide-react';
import { useRouter, usePathname } from 'next/navigation';
import { useSidebar } from './SidebarProvider';
import type { CurrentMeeting } from '@/components/Sidebar/SidebarProvider';
import { ConfirmationModal } from '../ConfirmationModel/confirmation-modal';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { SettingTabs } from '../SettingTabs';
import { TranscriptModelProps } from '@/components/TranscriptSettings';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { toast } from 'sonner';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useImportDialog } from '@/contexts/ImportDialogContext';

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from '../ui/button';

import { MessageToast } from '../MessageToast';
import Logo from '../Logo';
import Info from '../Info';
import { UserGuideButton } from '../UserGuide';
import { ComplianceNotification } from '../ComplianceNotification';
import { Input } from '../ui/input';
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from '../ui/input-group';
import { formatMeetingListDate } from '@/lib/formatMeetingListDate';

interface SidebarItem {
  id: string;
  title: string;
  type: 'folder' | 'file';
  children?: SidebarItem[];
  created_at?: string;
}

const Sidebar: React.FC = () => {
  const router = useRouter();
  const pathname = usePathname();
  const {
    currentMeeting,
    setCurrentMeeting,
    sidebarItems,
    isCollapsed,
    toggleCollapse,
    handleRecordingToggle,
    searchTranscripts,
    searchResults,
    isSearching,
    meetings,
    setMeetings,
    serverAddress
  } = useSidebar();

  // Get recording state from RecordingStateContext (single source of truth)
  const { isRecording } = useRecordingState();
  const { openImportDialog } = useImportDialog();
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set(['meetings']));
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [showModelSettings, setShowModelSettings] = useState(false);
  const [modelConfig, setModelConfig] = useState<ModelConfig>({
    provider: 'openai',
    model: '',
    apiKey: null,
  });
  const [transcriptModelConfig, setTranscriptModelConfig] = useState<TranscriptModelProps>({
    provider: 'asr',
    model: 'zipformer-vi-30m',
  });
  const [settingsSaveSuccess, setSettingsSaveSuccess] = useState<boolean | null>(null);

  // State for edit modal
  const [editModalState, setEditModalState] = useState<{ isOpen: boolean; meetingId: string | null; currentTitle: string }>({
    isOpen: false,
    meetingId: null,
    currentTitle: ''
  });
  const [editingTitle, setEditingTitle] = useState<string>('');

  // Ensure 'meetings' folder is always expanded
  useEffect(() => {
    if (!expandedFolders.has('meetings')) {
      const newExpanded = new Set(expandedFolders);
      newExpanded.add('meetings');
      setExpandedFolders(newExpanded);
    }
  }, [expandedFolders]);

  // useEffect(() => {
  //   if (settingsSaveSuccess !== null) {
  //     const timer = setTimeout(() => {
  //       setSettingsSaveSuccess(null);
  //     }, 3000);
  //   }
  // }, [settingsSaveSuccess]);


  const [deleteModalState, setDeleteModalState] = useState<{ isOpen: boolean; itemId: string | null }>({ isOpen: false, itemId: null });

  useEffect(() => {
    // Note: Don't set hardcoded defaults - let DB be the source of truth
    const fetchModelConfig = async () => {
      // Only make API call if serverAddress is loaded
      if (!serverAddress) {
        console.log('Waiting for server address to load before fetching model config');
        return;
      }

      try {
        const data = await invoke('api_get_model_config') as any;
        if (data && data.provider !== null) {
          // Fetch API key if not included and provider requires it
          if (data.provider !== 'custom-openai' && !data.apiKey) {
            try {
              const apiKeyData = await invoke('api_get_api_key', {
                provider: data.provider
              }) as string;
              data.apiKey = apiKeyData;
            } catch (err) {
              console.error('Failed to fetch API key:', err);
            }
          }
          setModelConfig(data);
        }
      } catch (error) {
        console.error('Failed to fetch model config:', error);
      }
    };

    fetchModelConfig();
  }, [serverAddress]);


  useEffect(() => {
    // Note: Don't set hardcoded defaults - let DB be the source of truth
    const fetchTranscriptSettings = async () => {
      // Only make API call if serverAddress is loaded
      if (!serverAddress) {
        console.log('Waiting for server address to load before fetching transcript settings');
        return;
      }

      try {
        const data = await invoke('api_get_transcript_config') as { live?: { model?: string } };
        if (data?.live?.model) {
          setTranscriptModelConfig({
            provider: 'asr',
            model: data.live.model,
            apiKey: null,
          });
        }
      } catch (error) {
        console.error('Failed to fetch transcript settings:', error);
      }
    };
    fetchTranscriptSettings();
  }, [serverAddress]);

  // Listen for model config updates from other components
  useEffect(() => {
    const setupListener = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      const unlisten = await listen<ModelConfig>('model-config-updated', (event) => {
        console.log('Sidebar received model-config-updated event:', event.payload);
        setModelConfig(event.payload);
      });

      return unlisten;
    };

    let cleanup: (() => void) | undefined;
    setupListener().then(fn => cleanup = fn);

    return () => {
      cleanup?.();
    };
  }, []);



  // Handle model config save
  const handleSaveModelConfig = async (config: ModelConfig) => {
    try {
      await invoke('api_save_model_config', {
        provider: config.provider,
        model: config.model,
        apiKey: config.apiKey,
        fallbackModelsJson: config.fallbackModels ? JSON.stringify(config.fallbackModels) : null,
      });

      setModelConfig(config);
      console.log('Model config saved successfully');
      setSettingsSaveSuccess(true);

      // Emit event to sync other components
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', config);

      // Track settings change
      await Analytics.trackSettingsChanged('model_config', `${config.provider}_${config.model}`);
    } catch (error) {
      console.error('Error saving model config:', error);
      setSettingsSaveSuccess(false);
    }
  };

  const handleSaveTranscriptConfig = async (updatedConfig?: TranscriptModelProps) => {
    try {
      const configToSave = updatedConfig || transcriptModelConfig;
      const payload = {
        provider: configToSave.provider,
        model: configToSave.model,
        apiKey: configToSave.apiKey ?? null
      };
      console.log('Saving transcript config with payload:', payload);

      await invoke('api_save_transcript_config', {
        provider: payload.provider,
        model: payload.model,
        apiKey: payload.apiKey,
      });


      setSettingsSaveSuccess(true);

      // Track settings change
      const transcriptConfigToSave = updatedConfig || transcriptModelConfig;
      await Analytics.trackSettingsChanged('transcript_config', `${transcriptConfigToSave.provider}_${transcriptConfigToSave.model}`);
    } catch (error) {
      console.error('Failed to save transcript config:', error);
      setSettingsSaveSuccess(false);
    }
  };

  // Handle search input changes
  const handleSearchChange = useCallback(async (value: string) => {
    setSearchQuery(value);

    // If search query is empty, just return to normal view
    if (!value.trim()) return;

    // Search through transcripts
    await searchTranscripts(value);

    // Make sure the meetings folder is expanded when searching
    if (!expandedFolders.has('meetings')) {
      const newExpanded = new Set(expandedFolders);
      newExpanded.add('meetings');
      setExpandedFolders(newExpanded);
    }
  }, [expandedFolders, searchTranscripts]);

  // Combine search results with sidebar items
  const filteredSidebarItems = useMemo(() => {
    if (!searchQuery.trim()) return sidebarItems;

    // If we have search results, highlight matching meetings
    if (searchResults.length > 0) {
      // Get the IDs of meetings that matched in transcripts
      const matchedMeetingIds = new Set(searchResults.map(result => result.id));

      return sidebarItems
        .map(folder => {
          // Always include folders in the results
          if (folder.type === 'folder') {
            if (!folder.children) return folder;

            // Filter children based on search results or title match
            const filteredChildren = folder.children.filter(item => {
              // Include if the meeting ID is in our search results
              if (matchedMeetingIds.has(item.id)) return true;

              // Or if the title matches the search query
              return item.title.toLowerCase().includes(searchQuery.toLowerCase());
            });

            return {
              ...folder,
              children: filteredChildren
            };
          }

          // For non-folder items, check if they match the search
          return (matchedMeetingIds.has(folder.id) ||
            folder.title.toLowerCase().includes(searchQuery.toLowerCase()))
            ? folder : undefined;
        })
        .filter((item): item is SidebarItem => item !== undefined); // Type-safe filter
    } else {
      // Fall back to title-only filtering if no transcript results
      return sidebarItems
        .map(folder => {
          // Always include folders in the results
          if (folder.type === 'folder') {
            if (!folder.children) return folder;

            // Filter children based on search query
            const filteredChildren = folder.children.filter(item =>
              item.title.toLowerCase().includes(searchQuery.toLowerCase())
            );

            return {
              ...folder,
              children: filteredChildren
            };
          }

          // For non-folder items, check if they match the search
          return folder.title.toLowerCase().includes(searchQuery.toLowerCase()) ? folder : undefined;
        })
        .filter((item): item is SidebarItem => item !== undefined); // Type-safe filter
    }
  }, [sidebarItems, searchQuery, searchResults, expandedFolders]);


  const handleDelete = async (itemId: string) => {
    console.log('Deleting item:', itemId);
    const payload = {
      meetingId: itemId
    };

    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('api_delete_meeting', {
        meetingId: itemId,
      });
      console.log('Meeting deleted successfully');
      const updatedMeetings = meetings.filter((m: CurrentMeeting) => m.id !== itemId);
      setMeetings(updatedMeetings);

      // Track meeting deletion
      Analytics.trackMeetingDeleted(itemId);

      // Show success toast
      toast.success('Đã xóa cuộc họp', {
        description: 'Toàn bộ dữ liệu liên quan đã được gỡ bỏ'
      });

      // If deleting the active meeting, navigate to home
      if (currentMeeting?.id === itemId) {
        setCurrentMeeting({ id: 'intro-call', title: '+ Cuộc họp mới' });
        router.push('/');
      }
    } catch (error) {
      console.error('Failed to delete meeting:', error);
      toast.error("Xóa cuộc họp thất bại", {
        description: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const handleDeleteConfirm = () => {
    if (deleteModalState.itemId) {
      handleDelete(deleteModalState.itemId);
    }
    setDeleteModalState({ isOpen: false, itemId: null });
  };

  // Handle modal editing of meeting names
  const handleEditStart = (meetingId: string, currentTitle: string) => {
    setEditModalState({
      isOpen: true,
      meetingId: meetingId,
      currentTitle: currentTitle
    });
    setEditingTitle(currentTitle);
  };

  const handleEditConfirm = async () => {
    const newTitle = editingTitle.trim();
    const meetingId = editModalState.meetingId;

    if (!meetingId) return;

    // Prevent empty titles
    if (!newTitle) {
      toast.error("Tiêu đề cuộc họp không được để trống");
      return;
    }

    try {
      await invoke('api_save_meeting_title', {
        meetingId: meetingId,
        title: newTitle,
      });

      // Update local state
      const updatedMeetings = meetings.map((m: CurrentMeeting) =>
        m.id === meetingId ? { ...m, title: newTitle } : m
      );
      setMeetings(updatedMeetings);

      // Update current meeting if it's the one being edited
      if (currentMeeting?.id === meetingId) {
        setCurrentMeeting({ id: meetingId, title: newTitle });
      }

      // Track the edit
      Analytics.trackButtonClick('edit_meeting_title', 'sidebar');

      toast.success("Đã cập nhật tiêu đề cuộc họp");

      // Close modal and reset state
      setEditModalState({ isOpen: false, meetingId: null, currentTitle: '' });
      setEditingTitle('');
    } catch (error) {
      console.error('Failed to update meeting title:', error);
      toast.error("Cập nhật tiêu đề thất bại", {
        description: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const handleEditCancel = () => {
    setEditModalState({ isOpen: false, meetingId: null, currentTitle: '' });
    setEditingTitle('');
  };

  const toggleFolder = (folderId: string) => {
    // Normal toggle behavior for all folders
    const newExpanded = new Set(expandedFolders);
    if (newExpanded.has(folderId)) {
      newExpanded.delete(folderId);
    } else {
      newExpanded.add(folderId);
    }
    setExpandedFolders(newExpanded);
  };

  // Expose setShowModelSettings to window for Rust tray to call
  useEffect(() => {
    (window as any).openSettings = () => {
      setShowModelSettings(true);
    };

    // Cleanup on unmount
    return () => {
      delete (window as any).openSettings;
    };
  }, []);

  const renderCollapsedIcons = () => {
    if (!isCollapsed) return null;

    const isHomePage = pathname === '/';
    const isMeetingPage = pathname?.includes('/meeting-details');
    const isSettingsPage = pathname === '/settings';

    return (
      <TooltipProvider>
        <div className="flex flex-col items-center space-y-4 mt-4">
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => router.push('/')}
                className={`p-2 rounded-md transition-colors duration-150 ${isHomePage ? 'bg-secondary text-foreground' : 'hover:bg-secondary'
                  }`}
              >
                <Home className="w-5 h-5 text-muted-foreground" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Trang chủ</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => {
                  if (isCollapsed) toggleCollapse();
                  toggleFolder('meetings');
                }}
                className={`p-2 rounded-md transition-colors duration-150 ${isMeetingPage ? 'bg-secondary text-foreground' : 'hover:bg-secondary'
                  }`}
              >
                <NotebookPen className="w-5 h-5 text-muted-foreground" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Ghi chú cuộc họp</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={handleRecordingToggle}
                disabled={isRecording}
                className={`p-2 ${isRecording ? 'bg-destructive cursor-not-allowed' : 'bg-destructive hover:bg-destructive/90'} rounded-md transition-colors duration-150`}
              >
                {isRecording ? (
                  <Square className="w-5 h-5 text-destructive-foreground" />
                ) : (
                  <Mic className="w-5 h-5 text-destructive-foreground" />
                )}
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>{isRecording ? "Đang ghi âm..." : "Bắt đầu ghi âm"}</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => openImportDialog()}
                className="p-2 rounded-md transition-colors duration-150 bg-primary/10 hover:bg-primary/15"
              >
                <Upload className="w-5 h-5 text-primary" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Nhập file âm thanh</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={() => router.push('/settings')}
                className={`p-2 rounded-md transition-colors duration-150 ${isSettingsPage ? 'bg-secondary text-foreground' : 'hover:bg-secondary'
                  }`}
              >
                <Settings className="w-5 h-5 text-muted-foreground" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Cài đặt</p>
            </TooltipContent>
          </Tooltip>

          <Info isCollapsed={isCollapsed} />
          <UserGuideButton isCollapsed={isCollapsed} />
        </div>
      </TooltipProvider>
    );
  };

  // Find matching transcript snippet for a meeting item
  const findMatchingSnippet = (itemId: string) => {
    if (!searchQuery.trim() || !searchResults.length) return null;
    return searchResults.find(result => result.id === itemId);
  };

  const renderItem = (item: SidebarItem, depth = 0) => {
    const isExpanded = expandedFolders.has(item.id);
    const paddingLeft = `${depth * 12 + 12}px`;
    const isActive = item.type === 'file' && currentMeeting?.id === item.id;
    const isMeetingItem = item.id.includes('-') && !item.id.startsWith('intro-call');

    // Check if this item has a matching transcript snippet
    const matchingResult = isMeetingItem ? findMatchingSnippet(item.id) : null;
    const hasTranscriptMatch = !!matchingResult;

    if (isCollapsed) return null;

    return (
      <div key={item.id}>
        <div
          className={`flex items-center min-w-0 transition-colors duration-150 group ${item.type === 'folder' && depth === 0
            ? 'px-4 pt-4 pb-1.5 text-[11px] font-semibold text-muted-foreground'
            : `px-2.5 py-2 rounded-r-md border-l-2 ${isActive
                ? 'border-primary bg-paper-2'
                : hasTranscriptMatch
                  ? 'border-transparent bg-primary/5'
                  : 'border-transparent hover:bg-secondary'
              } cursor-pointer`
            }`}
          style={item.type === 'file' || (item.type === 'folder' && depth === 0) ? undefined : { paddingLeft }}
          onClick={() => {
            if (item.type === 'folder') {
              toggleFolder(item.id);
            } else {
              setCurrentMeeting({ id: item.id, title: item.title });
              const basePath = item.id.startsWith('intro-call') ? '/' :
                item.id.includes('-') ? `/meeting-details?id=${item.id}` : `/notes/${item.id}`;
              router.push(basePath);
            }
          }}
        >
          {item.type === 'folder' ? (
            <>
              {item.id === 'meetings' ? (
                <Calendar className="w-4 h-4 mr-2" />
              ) : item.id === 'notes' ? (
                <Calendar className="w-4 h-4 mr-2" />
              ) : null}
              <span className={depth === 0 ? "" : "font-medium"}>{item.title}</span>
              <div className="ml-auto">
                {isExpanded ? (
                  <ChevronDown className="w-4 h-4 text-muted-foreground" />
                ) : (
                  <ChevronRight className="w-4 h-4 text-muted-foreground" />
                )}
              </div>
              {searchQuery && item.id === 'meetings' && isSearching && (
                <span className="ml-2 text-xs text-primary animate-pulse">Đang tìm...</span>
              )}
            </>
          ) : (
            <div className="relative flex flex-col w-full min-w-0">
              <div className="flex items-start gap-2 w-full min-w-0">
                {!isMeetingItem && (
                  <Plus className="w-3.5 h-3.5 shrink-0 mt-0.5 text-primary" />
                )}
                <div className="flex-1 min-w-0">
                  <span
                    className={`block truncate text-sm leading-snug ${
                      isActive ? 'font-medium text-ink' : 'font-normal text-ink'
                    }`}
                    title={item.title}
                  >
                    {item.title}
                  </span>
                  {item.created_at && (
                    <span className="block mt-1 font-mono text-[10px] leading-none tabular-nums tracking-[0.04em] text-ink-2 truncate">
                      {formatMeetingListDate(item.created_at)}
                    </span>
                  )}
                </div>
                {isMeetingItem && (
                  <div
                    className={`absolute right-0 top-0 flex items-center gap-0.5 pl-6 opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto bg-gradient-to-l to-transparent ${
                      isActive ? 'from-paper-2 from-50%' : 'from-rail from-50% group-hover:from-secondary'
                    }`}
                  >
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleEditStart(item.id, item.title);
                      }}
                      className="hover:text-primary p-1 rounded-md hover:bg-primary/10 shrink-0 text-ink-2"
                      aria-label="Sửa tiêu đề cuộc họp"
                    >
                      <Pencil className="w-3.5 h-3.5" />
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setDeleteModalState({ isOpen: true, itemId: item.id });
                      }}
                      className="hover:text-destructive p-1 rounded-md hover:bg-destructive/10 shrink-0 text-ink-2"
                      aria-label="Xóa cuộc họp"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                )}
              </div>

              {/* Transcript match snippet */}
              {hasTranscriptMatch && matchingResult && (
                <div className="mt-1.5 text-[11px] text-ink-2 bg-primary/5 px-2 py-1 rounded-md border border-rule line-clamp-2">
                  <span className="font-medium text-primary">Tìm thấy: </span>
                  {matchingResult.matchContext}
                </div>
              )}
            </div>
          )}
        </div>
        {item.type === 'folder' && isExpanded && item.children && (
          <div className="ml-1">
            {item.children.map(child => renderItem(child, depth + 1))}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="fixed top-0 left-0 h-screen z-40">
      <div
        className={`h-screen bg-rail border-r border-rule flex flex-col transition-all duration-[var(--dur-short)] ease-[var(--ease-out)] ${isCollapsed ? 'w-16' : 'w-64'
          }`}
      >
        <div className="flex-shrink-0 border-b border-rule">
          {isCollapsed ? (
            <div className="flex flex-col items-center gap-2 py-3 px-1">
              <Logo isCollapsed={isCollapsed} />
              <button
                onClick={toggleCollapse}
                className="p-1.5 rounded-md text-ink-2 hover:bg-secondary hover:text-foreground"
                aria-label="Mở rộng sidebar"
              >
                <ChevronsRight className="w-4 h-4" />
              </button>
            </div>
          ) : (
            <div className="p-3 pb-2.5">
              <div className="flex items-start justify-between gap-1">
                <Logo isCollapsed={isCollapsed} />
                <button
                  onClick={toggleCollapse}
                  className="mt-1 p-1.5 rounded-md text-ink-2 hover:bg-secondary hover:text-foreground shrink-0"
                  aria-label="Thu gọn sidebar"
                >
                  <ChevronsLeft className="w-4 h-4" />
                </button>
              </div>
              <p className="px-1 mb-2.5 font-mono text-[10px] tracking-[0.12em] uppercase text-ink-2">
                Thư ký cuộc họp
              </p>
              <InputGroup className="h-9 bg-paper-2 border-rule shadow-[var(--shadow-whisper)]">
                <InputGroupAddon className="text-ink-2">
                  <SearchIcon className="size-4" aria-hidden />
                </InputGroupAddon>
                <InputGroupInput
                  type="search"
                  placeholder="Tìm kiếm…"
                  aria-label="Tìm kiếm nội dung cuộc họp"
                  autoComplete="off"
                  value={searchQuery}
                  onChange={(e) => handleSearchChange(e.target.value)}
                  className="text-sm placeholder:text-ink-2 [&::-webkit-search-cancel-button]:hidden"
                />
                {searchQuery &&
                  <InputGroupAddon align={'inline-end'}>
                    <InputGroupButton
                      onClick={() => handleSearchChange('')}
                      aria-label="Xóa tìm kiếm"
                    >
                      <X />
                    </InputGroupButton>
                  </InputGroupAddon>
                }
              </InputGroup>
            </div>
          )}
        </div>

        {/* Main content - scrollable area */}
        <div className="flex-1 flex flex-col min-h-0">
          {/* Fixed navigation items */}
          <div className="flex-shrink-0">
            {!isCollapsed && (
              <div
                onClick={() => router.push('/')}
                className={`flex items-center gap-2 mx-2 mt-2 pl-2.5 pr-3 py-2 rounded-r-md text-sm font-medium cursor-pointer border-l-2 transition-colors duration-150 ${
                  pathname === '/'
                    ? 'border-primary bg-paper-2 text-ink'
                    : 'border-transparent text-muted-foreground hover:bg-secondary hover:text-foreground'
                }`}
              >
                <Home className="w-4 h-4 shrink-0" />
                <span>Trang chủ</span>
              </div>
            )}
          </div>

          {/* Content area */}
          <div className="flex-1 flex flex-col min-h-0">
            {renderCollapsedIcons()}
            {/* Meeting Notes folder header - fixed */}
            {!isCollapsed && (
              <div
                className="flex-1 flex flex-col min-h-0"
              >
                <div className="flex-shrink-0">
                  {filteredSidebarItems.filter(item => item.type === 'folder').map(item => (
                    <div key={item.id}>
                      <div className="flex items-center gap-1.5 px-4 pt-4 pb-1.5">
                        <NotebookPen className="w-3 h-3 text-muted-foreground shrink-0" />
                        <span className="font-mono text-[10px] font-medium uppercase tracking-[0.12em] text-ink-2">
                          {item.title}
                        </span>
                        {searchQuery && item.id === 'meetings' && isSearching && (
                          <span className="ml-1 text-[10px] text-primary animate-pulse">Đang tìm...</span>
                        )}
                      </div>
                    </div>
                  ))}
                </div>

                <div className="flex-1 overflow-y-auto custom-scrollbar min-h-0">
                  {filteredSidebarItems
                    .filter(item => item.type === 'folder' && expandedFolders.has(item.id) && item.children)
                    .map(item => (
                      <div key={`${item.id}-children`} className="px-2">
                        {item.children!.map(child => renderItem(child, 1))}
                      </div>
                    ))}
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        {!isCollapsed && (
          <div
            className="flex-shrink-0 p-2.5 border-t border-rule space-y-1"
          >
            {/* Primary: Recording */}
            <button
              onClick={handleRecordingToggle}
              disabled={isRecording}
              className={`w-full flex items-center justify-center gap-2 px-3 py-2 text-sm font-medium text-destructive-foreground rounded-md transition-colors ${
                isRecording
                  ? 'bg-destructive/70 cursor-not-allowed'
                  : 'bg-destructive hover:bg-destructive/90'
              }`}
            >
              {isRecording ? (
                <><Square className="w-3.5 h-3.5" fill="currentColor" /><span>Đang ghi âm...</span></>
              ) : (
                <><Mic className="w-3.5 h-3.5" /><span>Bắt đầu ghi âm</span></>
              )}
            </button>

            {/* Import file */}
            <button
              onClick={() => openImportDialog()}
              className="w-full flex items-center justify-center gap-2 px-3 py-2 text-sm font-medium text-primary border border-primary bg-transparent hover:bg-primary/10 rounded-md transition-colors"
            >
              <Upload className="w-3.5 h-3.5 shrink-0" />
              <span>Nhập file âm thanh</span>
            </button>

            {/* Settings */}
            <button
              onClick={() => router.push('/settings')}
              className={`w-full flex items-center justify-center gap-2 px-3 py-2 text-sm font-medium rounded-md transition-colors ${
                pathname === '/settings'
                  ? 'bg-secondary text-foreground'
                  : 'text-foreground bg-transparent hover:bg-secondary'
              }`}
            >
              <Settings className="w-3.5 h-3.5 shrink-0" />
              <span>Cài đặt</span>
            </button>

            <Info isCollapsed={isCollapsed} />
            <UserGuideButton isCollapsed={isCollapsed} />
          </div>
        )}
      </div>

      {/* Confirmation Modal for Delete */}
      <ConfirmationModal
        isOpen={deleteModalState.isOpen}
        text="Bạn có chắc muốn xóa cuộc họp này không? Hành động này không thể hoàn tác."
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteModalState({ isOpen: false, itemId: null })}
      />

      {/* Edit Meeting Title Modal */}
      <Dialog open={editModalState.isOpen} onOpenChange={(open) => {
        if (!open) handleEditCancel();
      }}>
        <DialogContent className="min-w-0 w-full max-w-md gap-0 overflow-hidden p-0">
          <DialogHeader className="space-y-0 px-5 pb-4 pt-5 pr-12 text-left">
            <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-ink-2">
              Cuộc họp
            </p>
            <DialogTitle className="mt-1 text-base font-semibold tracking-[-0.02em] text-ink">
              Chỉnh sửa tiêu đề cuộc họp
            </DialogTitle>
            <DialogDescription className="mt-1.5 text-xs text-ink-2">
              Tên này hiện trên danh sách và trang chi tiết.
            </DialogDescription>
          </DialogHeader>

          <div className="px-5 pb-4">
            <Input
              id="meeting-title"
              type="text"
              value={editingTitle}
              aria-label="Tiêu đề cuộc họp"
              onChange={(e) => setEditingTitle(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  handleEditConfirm();
                } else if (e.key === 'Escape') {
                  handleEditCancel();
                }
              }}
              placeholder="Nhập tiêu đề cuộc họp"
              autoFocus
            />
          </div>

          <DialogFooter className="border-t border-rule bg-paper px-5 py-3">
            <Button variant="outline" size="sm" onClick={handleEditCancel}>
              Hủy
            </Button>
            <Button size="sm" onClick={handleEditConfirm}>
              Lưu
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};

export default Sidebar;
