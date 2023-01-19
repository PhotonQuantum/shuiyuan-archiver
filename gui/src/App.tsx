import {useColorScheme} from "@mantine/hooks";
import {MantineProvider} from "@mantine/core";
import {Main} from "./Main";
import {RecoilRoot} from "recoil";

export const App = () => {
    const colorScheme = useColorScheme();

    return (
        <RecoilRoot>
            <MantineProvider theme={{colorScheme: colorScheme}} withGlobalStyles withNormalizeCSS>
                <Main/>
            </MantineProvider>
        </RecoilRoot>
    )
}