import { createContext, memo, useContext, type ReactNode } from 'react';
import { VStack } from '@astryxdesign/core/Layout';

const VisibilityContext = createContext(true);
export const usePageVisible = () => useContext(VisibilityContext);

// Keep visited tables in place; hidden pages receive fresh props when revisited.
export const RetainedView = memo(function RetainedView({ visible, children }: { visible: boolean; children: ReactNode }) {
  return <VisibilityContext.Provider value={visible}>
    <VStack hidden={!visible} style={{ display: visible ? 'contents' : 'none' }}>{children}</VStack>
  </VisibilityContext.Provider>;
}, (previous, next) => !previous.visible && !next.visible);
