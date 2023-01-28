import {useColorScheme} from "@mantine/hooks";
import {MantineProvider} from "@mantine/core";
import {Main} from "./Main";
import {RecoilRoot} from "recoil";
import {ModalsProvider} from "@mantine/modals";

export const App = () => {
    const colorScheme = useColorScheme();

    return (
      <RecoilRoot>
        <MantineProvider theme={{colorScheme: colorScheme}} withGlobalStyles withNormalizeCSS>
          <ModalsProvider>
            <Main/>
          </ModalsProvider>
        </MantineProvider>
      </RecoilRoot>
    )
}