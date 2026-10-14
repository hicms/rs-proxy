import type { ReactNode } from 'react';
import { Theme, useTheme } from '@astryxdesign/core/theme';
import { LayoutHeader, HStack, VStack } from '@astryxdesign/core/Layout';
import { Heading, Text } from '@astryxdesign/core/Text';
import { Button, type ButtonProps } from '@astryxdesign/core/Button';
import { themes } from '../astryx-themes';

export function Accent({ children }: { children: ReactNode }) {
  const { mode } = useTheme();
  return <Theme theme={themes.butter} mode={mode}>{children}</Theme>;
}

export function PrimaryButton(props: ButtonProps) {
  return <Accent><Button variant="primary" {...props} /></Accent>;
}

export function PageHeading({ title, description, actions, below }: {
  title: string; description?: string; actions?: ReactNode; below?: ReactNode;
}) {
  return <LayoutHeader hasDivider={false}><VStack gap={6} paddingBlockEnd={6}>
    <HStack gap={4} hAlign="between" vAlign="center" wrap="wrap">
      <VStack gap={2}><Heading level={1} weight="semibold">{title}</Heading>{description && <Text color="secondary">{description}</Text>}</VStack>
      {actions}
    </HStack>
    {below}
  </VStack></LayoutHeader>;
}
