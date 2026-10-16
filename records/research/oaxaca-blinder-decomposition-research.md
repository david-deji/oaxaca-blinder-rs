
The Oaxaca-Blinder Decomposition: A Theoretical and Algorithmic Guide


Section 1: Conceptual and Theoretical Foundations


1.1. Historical Provenance

The Oaxaca-Blinder (OB) decomposition, a cornerstone of quantitative analysis in the social sciences, has an intellectual lineage that predates its popularization in economics. The method's origins can be traced to the field of demography, where sociologist Evelyn M. Kitagawa developed a technique in 1955 to decompose the difference between two rates into components attributable to differences in rate structures and differences in population composition.1 This framing as a generalized method of standardization is fundamental to understanding its core logic.
The technique was introduced independently and concurrently into the economics literature in 1973 through two seminal papers by Alan S. Blinder and Ronald L. Oaxaca, which addressed the pressing social and economic issue of wage discrimination.3 Blinder's 1973 paper in
The Journal of Human Resources, titled "Wage Discrimination: Reduced Form and Structural Estimates," analyzed wage differentials between white and black men, and between white men and white women. His analysis concluded that discrimination was a primary driver, accounting for approximately 70% of the racial wage gap and the entirety of the gender wage gap among whites.6 Simultaneously, Oaxaca's 1973 paper in the
International Economic Review, "Male-Female Wage Differentials in Urban Labor Markets," which stemmed from his doctoral thesis at Princeton University, reached similar conclusions regarding the significant role of discrimination in explaining the wage gap between men and women.1
Due to their concurrent publication and similar approaches, the method is now widely known as the Oaxaca-Blinder (OB) decomposition, or sometimes the Kitagawa-Oaxaca-Blinder (KOB) decomposition, acknowledging its demographic roots.1 Later work by Oaxaca and Sierminska explicitly positioned the OB method as a generalization of Kitagawa's original framework, solidifying this intellectual connection.1

1.2. The Core Objective: Decomposing Mean Differences

At its core, the Oaxaca-Blinder decomposition is a statistical method designed to explain the difference in the mean of a continuous outcome variable between two distinct groups.3 Let Group A represent a typically higher-outcome group (e.g., males in a wage study) and Group B represent a lower-outcome group (e.g., females). The method seeks to decompose the raw mean difference, denoted as $ \Delta \bar{Y} = \bar{Y}_A - \bar{Y}_B $, into two primary components.3
The "Explained" Component: This portion of the gap is attributed to differences in the average levels of observable, productivity-related characteristics between the two groups. These characteristics are often referred to as "endowments".3 Examples in a labor market context include differences in education levels, years of work experience, job tenure, industry, or occupation.3 This component quantifies how much of the gap is due to the fact that the two groups, on average, possess different levels of these characteristics.
The "Unexplained" Component: This portion of the gap is attributed to differences in the economic returns, or "prices," that each group receives for their endowments.3 In the context of the underlying regression models, this corresponds to differences in the estimated coefficients. This component captures the part of the gap that persists even after accounting for differences in observable characteristics.
It is crucial to understand that the "unexplained" component is, by definition, a residual. It captures not only the effects of differential returns to characteristics—often interpreted as discrimination—but also the aggregate impact of all unobserved or unmodeled variables. The initial papers by Blinder and Oaxaca were explicitly focused on "wage discrimination," which has led to a common but imprecise equation of the unexplained portion with a direct measure of discrimination.6 However, this interpretation is a significant oversimplification. The unexplained component is a catch-all for any factors not included in the model, such as differences in the quality of education (as opposed to years of schooling), unobserved skills, societal biases, or systemic discrimination.1 For instance, one analysis of disparities affecting Indigenous communities noted that the substantial unexplained portion likely reflected unobserved factors such as "systemic discrimination, cultural norms, colonisation, peer effects, and intergenerational trauma".3 Therefore, the interpretation of the unexplained component is highly sensitive to the specification of the underlying regression models. A well-specified model that includes a rich set of control variables will yield a more refined, and likely smaller, unexplained component.

1.3. The Counterfactual Framework

The decomposition is fundamentally a counterfactual exercise that leverages the estimated statistical relationships to simulate alternative outcomes.3 The method addresses questions of the form: "What would the average outcome for Group B be if they possessed the same average characteristics as Group A, but continued to face the same returns structure as their own group?".21
By constructing these counterfactual means, the decomposition isolates the effects of endowments from the effects of coefficients. For example, the "explained" part of a wage gap can be understood as the predicted change in the female wage if women were endowed with the same average human capital characteristics (education, experience) as men. The remaining, "unexplained" part represents the gap that would persist even in this counterfactual scenario. This counterfactual nature is what gives the method its explanatory power, allowing researchers to move beyond simply documenting a gap to analyzing its constituent parts. However, this power is predicated on the validity of the underlying statistical models and their assumptions, a topic explored in detail in Section 6.5

1.4. Modern Applications Beyond Wage Gaps

While the Oaxaca-Blinder decomposition was born from the study of labor market discrimination, its applicability is domain-agnostic and has expanded far beyond its original context.5 It is now a standard tool in the toolkit of applied researchers across the social and medical sciences for analyzing any mean outcome difference between two groups.16
Contemporary applications include:
Health Economics: Decomposing disparities in healthcare expenditures, self-rated health, and specific health outcomes between different socioeconomic, racial, or demographic groups.3
Public Health: Analyzing differences in obesity prevalence, smoking rates, or HIV/AIDS incidence across populations.25
Education: Investigating test score gaps between students from different socioeconomic backgrounds or school systems.27
Development Economics: Decomposing changes in poverty rates over time or between regions into components related to changes in endowments (e.g., literacy) and returns to those endowments.18
This broad applicability underscores the method's fundamental utility as a descriptive and analytical tool for partitioning observed disparities into components related to observable characteristics and components related to the differential effects of those characteristics.

Section 2: The Mathematical Architecture of the Decomposition

The mathematical foundation of the Oaxaca-Blinder decomposition is built upon ordinary least squares (OLS) regression. The decomposition itself is an algebraic manipulation of the outputs from these regressions. This section provides the rigorous derivations necessary for a correct and robust implementation.

2.1. Foundation in Linear Regression

The procedure begins by estimating separate linear regression models for two mutually exclusive groups, designated as Group A and Group B. Typically, Group A is the group with the higher mean outcome. Let Y be the continuous outcome variable of interest and X be a column vector of k explanatory variables (predictors), including a constant term for the intercept.
The linear models for each group are specified as:
Group A: YiA​=XiA′​βA​+ϵiA​
Group B: YiB​=XiB′​βB​+ϵiB​
where i indexes the individual observations, βA​ and βB​ are k×1 vectors of population regression coefficients (including the intercept) for each group, and ϵiA​ and ϵiB​ are the error terms. A standard OLS assumption is that the error term has a conditional mean of zero: E[ϵA​∣XA​]=0 and E=0.4
A key property of OLS is that the regression line passes through the sample means of the variables. Therefore, the expected (or mean) value of the outcome variable for each group can be expressed as the linear prediction evaluated at the group-specific mean vector of the regressors.1
E=YˉA​=E[XA​]′βA​=XˉA′​βA​
E=YˉB​=E′βB​=XˉB′​βB​
Here, YˉA​ and YˉB​ are the sample means of the outcome variable, and XˉA​ and XˉB​ are the k×1 vectors of sample means of the explanatory variables for each group.
The overall mean outcome difference, ΔYˉ, which is the target of the decomposition, is thus given by:

ΔYˉ=YˉA​−YˉB​=XˉA′​β^​A​−XˉB′​β^​B​

where β^​A​ and β^​B​ are the OLS estimates of the coefficient vectors. This equation is the fundamental identity from which all forms of the decomposition are derived.4

2.2. The Three-Fold Decomposition

The three-fold decomposition is a complete and exact algebraic partitioning of the mean outcome gap. It is derived by adding and subtracting a counterfactual term to the fundamental identity. There are two equivalent ways to perform this decomposition, depending on which group is treated as the reference.
Derivation (using Group B's coefficients as the reference):
We start with the fundamental identity and add and subtract the term XˉA′​β^​B​:
$$ \Delta\bar{Y} = (\bar{X}_A'\hat{\beta}_A - \bar{X}_A'\hat{\beta}_B) + (\bar{X}_A'\hat{\beta}_B - \bar{X}_B'\hat{\beta}_B)
Rearrangingtheterms:
\Delta\bar{Y} = \bar{X}_A'(\hat{\beta}_A - \hat{\beta}_B) + (\bar{X}_A - \bar{X}_B)'\hat{\beta}_B $$
To arrive at the three-fold structure, we can further manipulate the first term, XˉA′​(β^​A​−β^​B​), by adding and subtracting XˉB′​(β^​A​−β^​B​):
$$ \bar{X}_A'(\hat{\beta}_A - \hat{\beta}_B) = \bar{X}_B'(\hat{\beta}_A - \hat{\beta}_B) + (\bar{X}_A - \bar{X}_B)'(\hat{\beta}_A - \hat{\beta}_B) $$
Substituting this back into the equation for ΔYˉ yields the standard three-fold decomposition 4:

$$ \Delta\bar{Y} = \underbrace{(\bar{X}_A - \bar{X}_B)'\hat{\beta}B}{\text{Endowments (E)}} + \underbrace{\bar{X}_B'(\hat{\beta}_A - \hat{\beta}B)}{\text{Coefficients (C)}} + \underbrace{(\bar{X}_A - \bar{X}_B)'(\hat{\beta}_A - \hat{\beta}B)}{\text{Interaction (I)}} $$
Interpretation of Components:
Endowments (E): This term represents the portion of the gap that is due to differences in the average observable characteristics between the groups (XˉA​−XˉB​), valued at the prices (coefficients) of Group B. It answers the counterfactual question: "How much would Group B's mean outcome change if they had the same endowments as Group A?".4
Coefficients (C): This term represents the portion of the gap due to differences in the returns to characteristics (β^​A​−β^​B​), valued at the endowment levels of Group B. It answers the counterfactual question: "How much would Group B's mean outcome change if they were compensated according to Group A's coefficient structure?".4
Interaction (I): This term accounts for the fact that differences in endowments and coefficients exist simultaneously. It captures the portion of the gap that arises because Group A has both different endowments and different returns compared to Group B. It can be interpreted as a "double advantage" for Group A (if positive) or a "double disadvantage".19
An alternative three-fold decomposition can be derived using Group A's coefficients as the reference, which would reallocate the interaction term differently.

2.3. The Two-Fold Decomposition

The two-fold decomposition is the more commonly used and reported version. It simplifies the three-fold structure by collapsing the components into two parts: an "explained" portion and an "unexplained" portion. This is achieved by introducing a hypothetical, non-discriminatory reference coefficient vector, denoted as β∗.4
General Formula:
The derivation starts again from the fundamental identity, this time adding and subtracting the term XˉB′​β∗:
$$ \Delta\bar{Y} = (\bar{X}_A'\hat{\beta}_A - \bar{X}_B'\beta^) - (\bar{X}_B'\hat{\beta}_B - \bar{X}_B'\beta^) $$
By adding and subtracting XˉA′​β∗ in the first term, we can rearrange to get the general two-fold decomposition:
$$ \Delta\bar{Y} = \underbrace{(\bar{X}A - \bar{X}B)'\beta^*}{\text{Explained (Q)}} + \underbrace{}{\text{Unexplained (U)}} $$
.4
Interpretation of Components:
Explained (Q): Also known as the "quantity effect" or "endowments effect," this term represents the part of the gap that is explained by group differences in predictors (XˉA​−XˉB​), valued at the "fair" or non-discriminatory returns defined by β∗.4
Unexplained (U): Also known as the "price effect" or "coefficients effect," this term represents the part of the gap due to the deviation of each group's actual coefficients from the reference structure β∗. The first part, XˉA′​(β^​A​−β∗), is often interpreted as the "advantage" accruing to Group A, while the second part, XˉB′​(β∗−β^​B​), is interpreted as the "disadvantage" faced by Group B, relative to the non-discriminatory norm.16 As previously noted, this component is often associated with discrimination but also subsumes all unobserved effects.4
The relationship between the two-fold and three-fold decompositions is not arbitrary; it is a direct consequence of how the interaction term is allocated. This has significant implications for implementation. For example, if we choose Group B's coefficients as the reference (i.e., β∗=β^​B​), the "explained" component of the two-fold decomposition, Q=(XˉA​−XˉB​)′β^​B​, becomes identical to the "endowments" component (E) of the three-fold decomposition. The "unexplained" component, U=XˉA′​(β^​A​−β^​B​), becomes the sum of the "coefficients" (C) and "interaction" (I) terms. Conversely, choosing Group A's coefficients as the reference (β∗=β^​A​) makes the unexplained component (U) equal to the coefficients component (C), while the explained component (Q) absorbs both the endowments (E) and interaction (I) terms.
This reveals a clear and computationally efficient path for a software library. The algorithm should first calculate the three fundamental components of the three-fold decomposition (E, C, and I). Then, based on the user's choice of reference coefficients for the two-fold decomposition, it can construct the "explained" and "unexplained" components by correctly allocating the interaction term. This approach is more robust and conceptually clearer than implementing separate logic for each decomposition type.

Section 3: The Indexing Problem: Selecting the Reference Coefficient Vector (β∗)

The choice of the non-discriminatory reference coefficient vector, β∗, is the most critical methodological decision in the two-fold Oaxaca-Blinder decomposition. This choice fundamentally alters the allocation of the gap between the "explained" and "unexplained" components. This issue is widely known in the literature as the "index number problem".4

3.1. Theoretical Implications of the Choice

The selection of β∗ is not merely a statistical technicality; it is a theoretical statement about the nature of the counterfactual world being modeled. The vector β∗ represents the set of returns to observable characteristics that would prevail in the absence of the differential treatment or structural differences being studied.4 For example, in a gender wage gap study:
Choosing the male coefficient vector as the reference implies a counterfactual where the female wage structure becomes identical to the male structure.
Choosing the female coefficient vector implies the reverse.
Choosing a pooled or average structure implies a counterfactual where both groups are subject to a new, common wage structure.
Since different choices of β∗ lead to different magnitudes for the explained and unexplained components, the policy conclusions drawn from the analysis can vary significantly.33 Therefore, a robust software implementation must provide users with a clear set of standard options and the flexibility to specify their own, while the accompanying documentation must explain the theoretical implications of each choice.

3.2. Common Specifications and Formulas

A comprehensive library should implement several widely accepted specifications for β∗. The following are the most common choices, each with a distinct underlying assumption.
Group A as Reference: Here, β∗=β^​A​. This assumes that the coefficient structure of the advantaged group (Group A) represents the non-discriminatory norm. The unexplained portion of the gap is then entirely attributed to the "disadvantage" of Group B relative to this norm.4
Group B as Reference: Here, β∗=β^​B​. This assumes the disadvantaged group's structure is the norm. The unexplained portion is then attributed to the "advantage" of Group A.4
Simple Average (Reimers, 1983): This specification uses an unweighted average of the two coefficient vectors: β∗=0.5β^​A​+0.5β^​B​. It provides a neutral midpoint and does not assume either group's structure is the "correct" one.29
Weighted Average (Cotton, 1988): This approach weights the coefficient vectors by their respective group sizes: β∗=wA​β^​A​+wB​β^​B​, where wA​=nA​/(nA​+nB​) and wB​=nB​/(nA​+nB​). This gives more influence to the wage structure of the larger group.4
Pooled Model (Neumark, 1988): This is one of the most common choices. It uses the coefficient vector, β^​pooled​, obtained from an OLS regression on the entire sample (both groups combined).29 This is interpreted as estimating an average, market-wide returns structure.
Crucial Implementation Detail: When implementing the Neumark decomposition, it is essential that the pooled regression model includes the group indicator variable as an additional regressor. Omitting the group indicator can lead to biased estimates of the other coefficients, as these coefficients will partially absorb the group-level difference in intercepts. This misspecification systematically overstates the contribution of the explained component and understates the unexplained component.4
The Oaxaca and Ransom (1994) Generalization:
Oaxaca and Ransom proposed a generalized framework that unifies all the above choices through a k×k weighting matrix, W.4 The reference coefficient vector is expressed as:
β∗=Wβ^​A​+(I−W)β^​B​

where I is the identity matrix.
This formulation provides an elegant and powerful architectural basis for a flexible software library. Instead of coding each specification for β∗ as a separate logical path, the algorithm can be designed to construct the appropriate weighting matrix W based on the user's selection and then apply a single, unified decomposition formula. This approach is cleaner, less error-prone, and more easily extensible to new specifications. For example:
Choosing Group A as the reference corresponds to W=I.
Choosing Group B as the reference corresponds to W=0.
The Reimers specification corresponds to W=0.5I.
The Neumark pooled model corresponds to W=(XA′​XA​+XB′​XB​)−1(XA′​XA​).4
A library built on this generalized framework can offer a high degree of flexibility while maintaining a simple and unified internal logic.

Table 1: Comparison of Reference Coefficient (β∗) Specifications

Specification Name
Proponent(s)
Formula for β∗
Equivalent Weighting Matrix (W)
Counterfactual Interpretation
Group A Reference
Oaxaca (1973); Blinder (1973)
β^​A​
I (Identity Matrix)
Assumes the advantaged group's returns structure is the non-discriminatory norm.
Group B Reference
Oaxaca (1973); Blinder (1973)
β^​B​
0 (Zero Matrix)
Assumes the disadvantaged group's returns structure is the non-discriminatory norm.
Simple Average
Reimers (1983)
0.5β^​A​+0.5β^​B​
0.5I
Assumes the non-discriminatory structure is a simple average of the two groups' returns.
Weighted Average
Cotton (1988)
wA​β^​A​+wB​β^​B​
wA​I
Assumes the non-discriminatory structure is an average weighted by group size.
Pooled Model
Neumark (1988)
β^​pooled​
(XA′​XA​+XB′​XB​)−1(XA′​XA​)
Assumes the non-discriminatory structure is represented by an average market-wide return.


Section 4: Detailed Decomposition: Attributing Effects to Individual Covariates

While the aggregate decomposition provides a high-level summary of the sources of the gap, a "detailed" decomposition is often required to understand the contribution of each individual explanatory variable. This allows researchers to pinpoint which characteristics (e.g., education vs. experience) and which returns to those characteristics are the primary drivers of the overall gap.

4.1. Decomposing the Explained Component

The aggregate explained component, Q=(XˉA​−XˉB​)′β∗, is a linear sum of the contributions from each of the k variables. Therefore, its detailed decomposition is mathematically straightforward.4 The contribution of the
j-th variable to the total explained gap is given by:

Qj​=(XˉjA​−XˉjB​)⋅βj∗​

where XˉjA​ and XˉjB​ are the mean values of the j-th predictor for Group A and Group B, respectively, and βj∗​ is the j-th element of the reference coefficient vector. The sum of these individual contributions over all j variables (from 1 to k) equals the total explained component, Q.

4.2. Decomposing the Unexplained Component

Similarly, the aggregate unexplained component, U, can also be decomposed into the contributions of individual variables. Using the formulation based on Group B as the reference for simplicity (where U=XˉA′​(β^​A​−β^​B​)), the contribution of the j-th variable to the total unexplained gap is:

Uj​=XˉjA​⋅(β^​jA​−β^​jB​)

This includes the contribution from the difference in the intercepts (j=0), which represents the portion of the gap that exists even when all other predictors are zero. The sum of these individual contributions equals the total unexplained component, U.4

4.3. The Identification Problem with Categorical Variables

A critical and often overlooked complication arises when performing a detailed decomposition of the unexplained component with categorical variables that are represented by a set of dummy (indicator) variables.35
The Problem: In a regression model, to avoid perfect multicollinearity (the "dummy variable trap"), one category of a categorical variable must be omitted and serves as the reference category. The estimated coefficients on the included dummy variables represent the difference in the outcome relative to this omitted base category. Consequently, the numerical values of these coefficients are entirely dependent on which category is chosen as the reference. This dependency carries over to the detailed decomposition. While the overall unexplained component remains invariant to the choice of the reference category, its attribution to the individual dummy variables is arbitrary and will change if a different base category is chosen.35
This is not a minor statistical curiosity; it is a fundamental identification problem that can render the detailed decomposition results for categorical variables meaningless. For example, a researcher analyzing an ethnic wage gap using a set of regional dummies might find a large "unexplained" effect for the "West" region when "North" is the omitted base. However, if they re-run the analysis with "South" as the base, the coefficient for "West"—and its calculated contribution to the gap—will change, potentially leading to completely different conclusions.
The Solution (Yun, 2005):
To resolve this identification problem, a normalization procedure must be applied to the coefficients of the categorical variables before performing the detailed decomposition. This procedure ensures that the results are invariant to the choice of the omitted base category.37 The goal is to express the effect of each category as a deviation from a common reference point, such as the grand mean effect of that categorical variable.
Algorithm for Normalization:
A practical method to achieve this invariance, as described in the literature, involves an adjustment to the estimated coefficients.36 For a categorical variable represented by a set of
m dummy variables (with one omitted):
Estimate the regression model as usual, obtaining the coefficients for the m−1 included dummy variables. The coefficient for the omitted category is implicitly zero.
Calculate an adjustment factor, a, which is the sum of the estimated coefficients for the m−1 dummies divided by the total number of categories, m.

a=m∑j=1m−1​β^​j​​
The "normalized" coefficient for each included dummy variable j is β^​j′​=β^​j​−a.
The "normalized" coefficient for the omitted base category is β^​base′​=0−a=−a.
The detailed decomposition of the unexplained component is then performed using these normalized coefficients. The sum of the contributions from this set of normalized coefficients will be invariant to the original choice of the omitted category.
Any serious software library for Oaxaca-Blinder decomposition must implement this normalization procedure. A failure to do so can lead users to draw scientifically invalid conclusions based on arbitrary modeling choices. The implementation should allow users to specify which variables are categorical, and the normalization should be applied automatically before the detailed decomposition of the unexplained component is computed.

Section 5: Statistical Inference and Standard Error Estimation

Point estimates from a decomposition analysis are insufficient for rigorous statistical inference. It is essential to calculate standard errors for the decomposed components to construct confidence intervals and perform hypothesis tests, thereby assessing the statistical significance of the results. Despite its importance, this aspect was often neglected in early applications of the method.21

5.1. The Importance of Stochastic Regressors

A crucial consideration in deriving correct variance formulas is the nature of the regressors. The components of the decomposition are functions of both the estimated coefficient vectors (β^​) and the sample mean vectors of the regressors (Xˉ). In most social science applications using survey data, both β^​ and Xˉ are random variables subject to sampling variation.21
Early or naive approaches to variance estimation often incorrectly assumed that the regressors were fixed (non-stochastic), which ignores the sampling variance of Xˉ. This assumption leads to standard errors that are biased downwards, potentially leading to incorrect inferences about the significance of the results.29 A correct implementation must account for both sources of variance: the uncertainty in the estimated coefficients and the uncertainty in the estimated means of the predictors.

5.2. Asymptotic Variance Estimation (The Delta Method)

The Delta Method is an analytical approach that provides an asymptotic approximation for the variance of a differentiable function of random variables.21 For the Oaxaca-Blinder decomposition, this involves deriving the variance of the complex functions that define the explained and unexplained components.
Ben Jann (2008) provides a comprehensive derivation of the variance formulas that correctly account for stochastic regressors. The foundational result is the formula for the variance of a single mean prediction, V(Xˉ′β^​). Assuming that the sample means Xˉ and the coefficient estimates β^​ are uncorrelated (which holds in a correctly specified OLS model where E[ϵ∣X]=0), the variance is given by the Goodman (1960) formula:
$$ V(\bar{X}'\hat{\beta}) = \hat{\beta}'V(\bar{X})\hat{\beta} + \bar{X}'V(\hat{\beta})\bar{X} + \text{tr}(V(\bar{X})V(\hat{\beta})) $$
.29
Here, V(Xˉ) is the variance-covariance matrix of the regressor means, and V(β^​) is the variance-covariance matrix of the coefficient estimates. These matrices can be estimated from the sample data. Building upon this, Jann derives the full variance-covariance matrix for all components of the two-fold and three-fold decompositions.
Implementing the Delta Method requires significant matrix algebra and the coding of these complex analytical formulas. However, once implemented, it is computationally very fast, making it suitable for exploratory analysis or applications with very large datasets. The Stata oaxaca command, for example, uses the Delta Method as its default approach for standard error calculation.21

5.3. Non-Parametric Estimation (Bootstrapping)

Bootstrapping is a resampling-based method that provides a robust, non-parametric alternative for estimating standard errors. It is computationally more intensive than the Delta Method but is conceptually simpler to implement and relies on fewer distributional assumptions.16
The procedure for calculating bootstrapped standard errors for the decomposition components is as follows:
Resampling: From the original dataset of size N, draw R independent bootstrap samples. Each bootstrap sample is of size N and is drawn with replacement from the original data. A typical value for R is in the range of 100 to 1,000.16
Re-estimation: For each of the R bootstrap samples, perform the entire Oaxaca-Blinder decomposition procedure. This involves re-estimating the group-specific regressions, re-calculating the mean vectors, and re-computing all the aggregate and detailed decomposition components. The results for each component from each of the R iterations are stored.
Standard Error Calculation: The bootstrapped standard error for any given decomposition component (e.g., the total unexplained effect) is simply the sample standard deviation of the R estimates of that component obtained in Step 2.16
This process automatically accounts for all sources of sampling variability (in both β^​ and Xˉ) without requiring the derivation of complex analytical variance formulas. It is particularly useful when the assumptions underlying the Delta Method might be violated or for more complex estimation procedures (e.g., those involving selection correction or non-linear models) where analytical variances are intractable.41
Given the trade-off between the Delta Method's computational speed and the bootstrap's implementation simplicity and robustness, a high-quality software library should offer both options to the user. This allows users to choose se_method="delta" for quick, iterative work and se_method="bootstrap" for final, publication-quality results where robustness is paramount.

Table 2: Algorithmic Steps for Bootstrapped Standard Errors

Step
Action
Description
1
Initialization
Set the number of bootstrap replications, R. Initialize empty storage arrays for each decomposition component to be analyzed (e.g., unexplained_estimates, explained_estimates).
2
Bootstrap Loop
Start a loop that iterates from i=1 to R.
3
Resample Data
Inside the loop, create a bootstrap sample by drawing N observations with replacement from the original dataset of size N.
4
Perform Decomposition
Using the bootstrap sample from Step 3, execute the full Oaxaca-Blinder decomposition algorithm. This includes splitting the data into groups, running OLS regressions, calculating means, and computing the desired decomposition components (e.g., current_unexplained, current_explained).
5
Store Results
Store the computed components from Step 4 into their respective storage arrays (e.g., unexplained_estimates[i] = current_unexplained).
6
End Loop
End the loop after R iterations.
7
Calculate Standard Errors
After the loop completes, calculate the standard deviation of the values in each storage array. For example, SE_unexplained = sd(unexplained_estimates). This is the bootstrapped standard error.


Section 6: Core Assumptions and Advanced Topics

The validity and interpretation of the Oaxaca-Blinder decomposition are contingent upon the assumptions of the underlying statistical models. Violations of these assumptions can lead to biased results and incorrect conclusions. A comprehensive understanding of these limitations is essential for both implementing the algorithm correctly and for interpreting its output responsibly.

6.1. The Linearity Assumption and Non-Linear Extensions

The standard OB decomposition is predicated on the assumption that the outcome variable can be adequately modeled by a linear OLS regression.4 If the true relationship between the outcome and the predictors is non-linear, the linear approximation may be poor, and the resulting decomposition can be misleading.42
For cases with binary or categorical outcome variables (e.g., employment status, health condition), linear models are often inappropriate. While extensions of the decomposition for non-linear models like Logit and Probit exist, they introduce significant complexity.42 In these models, the estimated coefficients do not represent marginal effects directly. Therefore, the decomposition cannot be applied to the coefficients themselves. Instead, it must be performed on the predicted probabilities or outcomes. This involves calculating average predicted probabilities for different counterfactual scenarios, which makes the detailed decomposition, in particular, more complex to compute and interpret.33 While beyond the scope of a basic library, implementing non-linear extensions is a key area for future development.

6.2. The Exogeneity Assumption and Omitted Variable Bias

Like all OLS-based methods, the OB decomposition relies on the crucial assumption of exogeneity: the explanatory variables (X) must be uncorrelated with the error term (ϵ). This assumption, E[ϵ∣X]=0, is violated if there are omitted variables that are correlated with both the included predictors and the outcome variable.6
When omitted variable bias is present, the estimated coefficients (β^​) are biased and inconsistent. This bias directly contaminates both the explained and unexplained components of the decomposition. The unexplained component is particularly affected, as it becomes a repository for the effects of all unobserved factors.21 For example, if "ambition" is an unobserved variable that affects both education (an included predictor) and wages (the outcome), its effect will be partially loaded onto the coefficient for education and will also contribute to the unexplained gap. This further reinforces the point that the unexplained component should not be naively equated with discrimination.

6.3. Endogeneity and Selection Bias

A specific and common violation of the exogeneity assumption occurs when group membership itself is endogenous. The standard OB decomposition assumes that individuals are exogenously assigned to Group A or Group B.47 However, in many real-world scenarios, individuals self-select into groups based on unobserved characteristics that also affect the outcome. Examples include choosing to join a union, migrating, or attaining a certain level of education.
This self-selection creates a correlation between the group indicator and the error term, a form of endogeneity known as selection bias.47 For instance, if more motivated individuals are more likely to join a union
and earn higher wages for reasons other than unionization, a simple comparison of union and non-union wages will be biased.
The Heckman Correction: A widely used method to address selection bias is the two-step procedure developed by James Heckman.48
Selection Equation: A first-stage probit model is estimated to predict the probability of an individual being in a particular group (e.g., being a union member) based on a set of identifying variables.
Correction Term: From this probit model, the Inverse Mills Ratio (IMR) is calculated for each observation. The IMR is a measure of the selection hazard for each individual.
Outcome Equation: The IMR is then included as an additional explanatory variable in the second-stage OLS wage regressions for each group. Its coefficient captures the effect of the selection bias, and its inclusion allows for unbiased estimates of the other coefficients.47
An advanced implementation of an OB library could include functionality to perform this two-step Heckman correction, allowing for more robust decompositions in the presence of selection bias.

6.4. Multicollinearity

The OB decomposition inherits the standard OLS sensitivity to multicollinearity—a high degree of linear correlation among the predictor variables.49 While perfect multicollinearity (e.g., the dummy variable trap) makes estimation impossible, near-multicollinearity can still pose problems. It inflates the standard errors of the coefficient estimates, making them unstable and imprecise.36
This instability does not bias the overall explained and unexplained components, but it severely undermines the reliability of the detailed decomposition. When two predictors are highly correlated, the model cannot precisely distinguish their individual effects, and their respective contributions to the gap will be unreliable. The library's documentation should advise users to diagnose multicollinearity (e.g., by examining Variance Inflation Factors, VIFs) in their underlying regression models before drawing strong conclusions from the detailed decomposition results.
Ultimately, the Oaxaca-Blinder decomposition is not a tool for causal inference in and of itself. It is a powerful descriptive or "accounting" method that re-organizes the information contained in OLS regressions.24 Its ability to support causal or meaningful counterfactual interpretations is entirely dependent on the causal validity of the underlying regression models. If the exogeneity assumptions required for OLS to estimate causal effects are met, then the decomposition can provide causal insights. If not, it remains a valid statistical decomposition of a mean difference, but its components cannot be interpreted as the causal effects of endowments and returns.23

Section 7: A Blueprint for Library Implementation

This section synthesizes the preceding theoretical and statistical analysis into a practical, high-level blueprint for designing and implementing a robust and user-friendly Oaxaca-Blinder decomposition software library.

7.1. Core Function Signature and Parameters

A modern statistical library should prioritize an expressive and intuitive user interface. A formula-based interface, inspired by implementations in R and Stata, is highly recommended for its clarity and power.32
Proposed Signature:
oaxaca_decompose(formula, data, reference_type="pooled", se_method="bootstrap", R=500,...)
Formula Specification:
The formula should allow for the specification of all core model components in a single, coherent string or object. A powerful syntax would be:
outcome ~ predictor1 + predictor2 | group_variable | categorical1 + categorical2
outcome ~ predictor1 + predictor2: The standard regression formula for the outcome model.
| group_variable: A separator followed by the name of the binary variable that defines Group A and Group B.
| categorical1 + categorical2: An optional second separator followed by the names of variables that should be treated as categorical for the purpose of normalization in the detailed decomposition.32

Table 3: API Blueprint for the oaxaca_decompose function

Parameter
Type
Default Value
Description
formula
String / Formula Object
Required
Specifies the outcome, predictors, group variable, and categorical variables using the syntax described above.
data
DataFrame / Array
Required
The input dataset containing all variables specified in the formula.
reference_type
String
"pooled"
Specifies the choice of reference coefficients (β∗). Options should include: "group_a", "group_b", "reimers", "cotton", "pooled".
se_method
String
"bootstrap"
Method for standard error calculation. Options: "delta" for the analytical Delta Method or "bootstrap" for non-parametric bootstrapping.
R
Integer
500
Number of bootstrap replications to perform if se_method="bootstrap". Ignored otherwise.
weights
String / Array
None
Optional. Specifies survey weights for weighted least squares (WLS) estimation.
selection_formula
String / Formula Object
None
Advanced. An optional formula for a first-stage Heckman selection model to correct for endogeneity.


7.2. Structure of the Output Object

The function should not return raw numbers but a well-structured object (e.g., a class instance) that organizes all results logically and provides convenient methods for inspection and visualization.32
Key Features of the Output Object:
A .summary() method to print a formatted, human-readable table of the main decomposition results.
A .plot() method to generate a bar chart visualizing the aggregate decomposition components, facilitating interpretation.31
Clearly named attributes to programmatically access all computed values, including point estimates and standard errors for both aggregate and detailed decompositions.
Access to the underlying fitted regression models for diagnostic purposes.

Table 4: Structure of the Returned Results Object

Attribute
Type
Description
.summary()
Method
Prints a formatted summary of the decomposition results.
.plot()
Method
Generates a bar chart visualization of the results.
.gap
Dictionary
Contains the mean outcomes for each group and the overall difference (y_a, y_b, diff).
.threefold
Dictionary
Contains the results of the three-fold decomposition. Includes sub-dictionaries for aggregate and detailed results, each with point estimates and standard errors for endowments, coefficients, and interaction.
.twofold
Dictionary
Contains the results of the two-fold decomposition for the chosen reference_type. Includes sub-dictionaries for aggregate and detailed results, each with point estimates and standard errors for explained and unexplained.
.models
Dictionary
Contains the fitted regression model objects for group_a, group_b, and pooled for user inspection.
.data_summary
Dictionary
Contains summary statistics used in the calculation, such as mean vectors (x_bar_a, x_bar_b) and observation counts (n_a, n_b).
.call_summary
Dictionary
Stores the parameters used in the function call (e.g., reference_type, se_method, R).


7.3. High-Level Algorithm (Pseudocode)

The following pseudocode outlines the core logic for the oaxaca_decompose function.



function oaxaca_decompose(formula, data,...params):
    // Step 1: Parse Inputs
    outcome, predictors, group_var, categorical_vars = parse_formula(formula)

    // Step 2: Partition Data
    data_A = data[data[group_var] == group_A_value]
    data_B = data[data[group_var] == group_B_value]

    // Step 3: Estimate Models
    model_A = OLS(outcome ~ predictors, data=data_A)
    model_B = OLS(outcome ~ predictors, data=data_B)
    model_pooled = OLS(outcome ~ predictors + group_var, data=data)
    
    beta_hat_A, V_beta_A = model_A.results()
    beta_hat_B, V_beta_B = model_B.results()
    beta_hat_pooled = model_pooled.coefficients()

    // Step 4: Calculate Means
    X_bar_A, V_X_bar_A = calculate_means_and_cov(data_A[predictors])
    X_bar_B, V_X_bar_B = calculate_means_and_cov(data_B[predictors])

    // Step 5: Normalize Coefficients (if applicable)
    if categorical_vars are specified:
        beta_hat_A = normalize_coeffs(beta_hat_A, categorical_vars)
        beta_hat_B = normalize_coeffs(beta_hat_B, categorical_vars)
        // Note: Normalization applies to detailed decomposition of U

    // Step 6: Compute Three-Fold Components
    E = (X_bar_A - X_bar_B)' * beta_hat_B
    C = X_bar_B' * (beta_hat_A - beta_hat_B)
    I = (X_bar_A - X_bar_B)' * (beta_hat_A - beta_hat_B)

    // Step 7: Compute Two-Fold Components
    W = construct_weighting_matrix(params.reference_type,...)
    beta_star = W * beta_hat_A + (I - W) * beta_hat_B
    Q = (X_bar_A - X_bar_B)' * beta_star
    U = (X_bar_A' * (beta_hat_A - beta_star)) + (X_bar_B' * (beta_star - beta_hat_B))

    // Step 8: Compute Detailed Decompositions
    detailed_E = elementwise_product(X_bar_A - X_bar_B, beta_hat_B)
    detailed_C = elementwise_product(X_bar_B, beta_hat_A - beta_hat_B)
    //... and so on for all other components, using normalized coeffs for U

    // Step 9: Estimate Standard Errors
    if params.se_method == "delta":
        SEs = calculate_delta_method_ses(beta_hat_A, V_beta_A, X_bar_A, V_X_bar_A,...)
    else if params.se_method == "bootstrap":
        SEs = calculate_bootstrap_ses(data, formula, params.R,...)

    // Step 10: Assemble and Return Output Object
    results = new OaxacaResultsObject()
    populate_results(results, E, C, I, Q, U, detailed_decomps, SEs,...)
    return results



Conclusion

The Oaxaca-Blinder decomposition is a powerful and versatile statistical method for analyzing mean outcome differences between groups. Originating from foundational work in demography and labor economics, it has evolved into a standard analytical tool across numerous disciplines. Its core strength lies in its ability to partition a raw gap into an "explained" component, attributable to differences in observable characteristics, and an "unexplained" component, driven by differences in the returns to those characteristics and other unobserved factors.
This report has provided a comprehensive theoretical and algorithmic guide intended to serve as a blueprint for the development of a robust software library. The key takeaways for such an implementation are:
Flexibility is Paramount: The library must offer users choices for the critical methodological decisions, particularly the selection of the reference coefficient vector (β∗) that defines the counterfactual, and the method for estimating standard errors (Delta Method vs. Bootstrapping).
Robustness is Non-Negotiable: A reliable implementation must address the nuanced statistical challenges inherent in the method. This includes correctly accounting for stochastic regressors in variance calculations and, most importantly, implementing a normalization procedure to resolve the identification problem for categorical variables in the detailed decomposition. Failure to address the latter can lead to arbitrary and misleading results.
Clarity in Interpretation is Essential: The documentation and output must guide users toward a correct interpretation. It should be made explicit that the "unexplained" component is a residual, not a direct measure of discrimination, and that the causal validity of the decomposition is entirely inherited from the underlying regression models.
By adhering to these principles, a software library can provide researchers with a powerful tool that is not only computationally correct but also encourages sound statistical practice, enabling deeper and more nuanced analyses of the factors driving disparities in society.
Sources des citations
Kitagawa–Oaxaca–Blinder decomposition - Wikipedia, consulté le septembre 14, 2025, https://en.wikipedia.org/wiki/Kitagawa%E2%80%93Oaxaca%E2%80%93Blinder_decomposition
Oaxaca-Blinder Meets Kitagawa: What Is the Link? - IZA - Institute of Labor Economics, consulté le septembre 14, 2025, https://docs.iza.org/dp16188.pdf
The Blinder–Oaxaca Decomposition for Linear Regression Models - ResearchGate, consulté le septembre 14, 2025, https://www.researchgate.net/publication/23780242_The_Blinder-Oaxaca_Decomposition_for_Linear_Regression_Models
The Blinder–Oaxaca decomposition for linear regression models - AgEcon Search, consulté le septembre 14, 2025, https://ageconsearch.umn.edu/record/122615/files/sjart_st0151.pdf
Decomposition Methods in Economics, consulté le septembre 14, 2025, https://economics.ubc.ca/wp-content/uploads/sites/38/2013/05/pdf_paper_thomas-lemieux-decomposition-methods-economics.pdf
Oaxaca-Blinder wage decomposition: Methods, critiques and applications. A literature review - SciELO Colombia, consulté le septembre 14, 2025, http://www.scielo.org.co/scielo.php?script=sci_arttext&pid=S2011-21062010000100006
Wage Discrimination: Reduced Form and Structural Estimates - ScienceOpen, consulté le septembre 14, 2025, https://www.scienceopen.com/document?vid=f396335b-a51c-43c1-ad65-a3577e50a4a3
Wage Discrimination: Reduced Form and Structural Estimates, consulté le septembre 14, 2025, https://ideas.repec.org/a/uwp/jhriss/v8y1973i4p436-455.html
ED081964 - Male-Female Wage Differentials in Urban Labor Markets., 1971-Jun - ERIC, consulté le septembre 14, 2025, https://eric.ed.gov/?id=ED081964
Male-Female Wage Differentials in Urban Labor Markets - IDEAS/RePEc, consulté le septembre 14, 2025, https://ideas.repec.org/a/ier/iecrev/v14y1973i3p693-709.html
Male-Female Wage Differentials in Urban Labor Markets - Princeton Dataspace, consulté le septembre 14, 2025, https://dataspace.princeton.edu/handle/88435/dsp012514nk49s
On the Decomposition of Wage Differentials - UMass Boston ScholarWorks, consulté le septembre 14, 2025, https://scholarworks.umb.edu/cgi/viewcontent.cgi?article=1025&context=econ_faculty_pubs
Oaxaca, R.L. (1973) Male-Female Wage Differentials in Urban Labor Markets. International Economic Review, 14, 693-709. - References - Scientific Research Publishing, consulté le septembre 14, 2025, https://www.scirp.org/reference/referencespapers?referenceid=1821038
Oaxaca, R. (1973). Male-Female Wage Differentials in Urban Labour Markets. International Economic Review, 14, 693-709. - References, consulté le septembre 14, 2025, https://www.scirp.org/reference/referencespapers?referenceid=3540756
Male-Female Wage Differentials in Urban Labor Markets - Library Network, consulté le septembre 14, 2025, https://imf.primo.exlibrisgroup.com/discovery/fulldisplay?docid=cdi_proquest_journals_1299980682&context=PC&vid=01TIMF_INST:Shared&lang=en&adaptor=Primo%20Central&tab=Everything&query=null%2C%2C2009&facet=citing%2Cexact%2Ccdi_FETCH-LOGICAL-c6208-e2e3d48d0b64c31abf063e9e7caa6a65cc023a9a3d4ecc011c6ed9e974169fb53&offset=0
Blinder-Oaxaca Decomposition in R - CRAN, consulté le septembre 14, 2025, https://cran.r-project.org/web/packages/oaxaca/vignettes/oaxaca.Rtex
Threefold (interaction) Blinder-Oaxaca decomposition for non-linear... - ResearchGate, consulté le septembre 14, 2025, https://www.researchgate.net/figure/Threefold-interaction-Blinder-Oaxaca-decomposition-for-non-linear-models-using-rural_fig5_353740560
Chapter 15 Decomposing changes in poverty into endowments and returns in Bangladesh {note-oaxaca} | South Asia Regional Micro Database (SARMD) User Guidelines - worldbank.github.io, consulté le septembre 14, 2025, https://worldbank.github.io/SARMD_guidelines/decomposing-changes-in-poverty-into-endowments-and-returns-in-bangladesh-note-oaxaca.html
Oaxaca Blinder Decomposition Interaction - Statalist, consulté le septembre 14, 2025, https://www.statalist.org/forums/forum/general-stata-discussion/general/1737352-oaxaca-blinder-decomposition-interaction
An Introduction to the Blinder-Oaxaca Decomposition - UDRC, consulté le septembre 14, 2025, https://udrc.ushe.edu/news/2021/research_skills/20210811BlinderOaxaca.html
The Blinder–Oaxaca decomposition for linear regression models, consulté le septembre 14, 2025, http://zamek415.free.fr/QEM2016%20wyk%20x%20Oaxaca.pdf
Blinder-Oaxaca Decomposition in R | Giacomo Vagni, consulté le septembre 14, 2025, https://giacomovagni.com/blog/2023/oaxaca/
MIT Graduate Labor Economics 14.662 Spring 2015 Lecture Note 1: Wage Density Decompositions, consulté le septembre 14, 2025, https://ocw.mit.edu/courses/14-662-labor-economics-ii-spring-2015/12ff98a88fe3005fec9f81351c40ab73_MIT14_662S15_lecnotes1.pdf
Decompositions: Accounting for Discrimination - The University of Sheffield, consulté le septembre 14, 2025, https://sheffield.ac.uk/media/37661/download?attachment
Oaxaca-Blinder decomposition of disparities in adolescent obesity: deconstructing both race and gender differences - PMC, consulté le septembre 14, 2025, https://pmc.ncbi.nlm.nih.gov/articles/PMC4792537/
Blinder-Oaxaca decomposition method - LSHTM Research Online, consulté le septembre 14, 2025, https://researchonline.lshtm.ac.uk/id/eprint/2159843/1/12889_2015_1607_MOESM1_ESM.pdf
USING THE OAXACA-BLINDER DECOMPOSITION TECHNIQUE TO ANALYZE LEARNING OUTCOMES CHANGES OVER TIME - USC, consulté le septembre 14, 2025, https://www.usc.es/economet/reviews/eers1134.pdf
Oaxaca-Blinder decomposition of changes in means and inequality: A simultaneous approach - Freakonometrics, consulté le septembre 14, 2025, https://freakonometrics.hypotheses.org/files/2023/03/Ineq-decomposition.pdf
Standard Errors for the Blinder--Oaxaca Decomposition - Stata, consulté le septembre 14, 2025, https://www.stata.com/meeting/3german/jann.pdf
Oaxaca decomposition technique : r/stata - Reddit, consulté le septembre 14, 2025, https://www.reddit.com/r/stata/comments/ljaz8w/oaxaca_decomposition_technique/
Package “oaxaca” for Blinder-Oaxaca Decomposition in R — Part 2: Features and Example, consulté le septembre 14, 2025, https://medium.com/@MarekHlavac/package-oaxaca-for-blinder-oaxaca-decomposition-in-r-part-2-features-and-example-12222e9a886d
oaxaca function - RDocumentation, consulté le septembre 14, 2025, https://www.rdocumentation.org/packages/oaxaca/versions/0.1.5/topics/oaxaca
An Extension of the Blinder-Oaxaca Decomposition Technique to Logit and Probit Models - IZA - Institute of Labor Economics, consulté le septembre 14, 2025, https://docs.iza.org/dp1917.pdf
Unexplained Gaps and Oaxaca-Blinder Decompositions, consulté le septembre 14, 2025, https://d-nb.info/994254229/34
A Simple Solution to the Identification Problem in Detailed Wage Decompositions - IZA - Institute of Labor Economics, consulté le septembre 14, 2025, https://repec.iza.org/dp836.pdf
Package “oaxaca” for Blinder-Oaxaca Decomposition in R — Part 1: Introduction and Theory, consulté le septembre 14, 2025, https://medium.com/@MarekHlavac/package-oaxaca-for-blinder-oaxaca-decomposition-in-r-part-1-introduction-and-theory-b8dafb085a31
Using normalized equations to solve the indetermination problem in the Oaxaca- Blinder decomposition, consulté le septembre 14, 2025, https://www.diw.de/sixcms/detail.php?id=60085
oaxaca-blinder decomposition and categorical variables - Statalist, consulté le septembre 14, 2025, https://www.statalist.org/forums/forum/general-stata-discussion/general/1444549-oaxaca-blinder-decomposition-and-categorical-variables
oaxaca.hlp, consulté le septembre 14, 2025, http://fmwww.bc.edu/repec/bocode/o/oaxaca.hlp
How can I estimate the standard error of transformed regression parameters in R using the delta method? | R FAQ - OARC Stats, consulté le septembre 14, 2025, https://stats.oarc.ucla.edu/r/faq/how-can-i-estimate-the-standard-error-of-transformed-regression-parameters-in-r-using-the-delta-method/
NBER TECHNICAL WORKING PAPER SERIES BOOTSTRAP-BASED IMPROVEMENTS FOR INFERENCE WITH CLUSTERED ERRORS A. Colin Cameron Jonah B. G, consulté le septembre 14, 2025, https://www.nber.org/system/files/working_papers/t0344/t0344.pdf
An Extension of the Blinder-Oaxaca Decomposition to Non-Linear Models - EconStor, consulté le septembre 14, 2025, https://www.econstor.eu/bitstream/10419/18600/1/DP_06_049.pdf
Decomposing Differences in the First Moment - IZA - Institute of Labor Economics, consulté le septembre 14, 2025, https://docs.iza.org/dp877.pdf
Heterogeneity in Health State Dependence of Utility - American Economic Association, consulté le septembre 14, 2025, https://www.aeaweb.org/conference/2013/retrieve.php?pdfid=375
AN EXTENSION OF THE BLINDER-OAXACA DECOMPOSITION TECHNIQUE TO LOGIT AND PROBIT MODELS - Yale Department of Economics, consulté le septembre 14, 2025, http://www.econ.yale.edu/growth_pdf/cdp873.pdf
Unexplained gaps and Oaxaca-Blinder decompositions - EconStor, consulté le septembre 14, 2025, https://www.econstor.eu/bitstream/10419/35398/1/599484799.pdf
A Semi-Parametric Approach to the Oaxaca-Blinder Decomposition - Levy Economics Institute of Bard College, consulté le septembre 14, 2025, https://www.levyinstitute.org/wp-content/uploads/2024/02/wp_930.pdf
A Semi-Parametric Approach to the Oaxaca–Blinder Decomposition with Continuous Group Variable and Self-Selection - MDPI, consulté le septembre 14, 2025, https://www.mdpi.com/2225-1146/7/2/28
Multicollinearity and misleading statistical results - PMC, consulté le septembre 14, 2025, https://pmc.ncbi.nlm.nih.gov/articles/PMC6900425/
Analyzing Health Equity: Explaining Differences Between Groups: Oaxaca Decomposition - World Bank, consulté le septembre 14, 2025, https://www.worldbank.org/content/dam/Worldbank/document/HDN/Health/HealthEquityCh12.pdf
Source code for statsmodels.stats.oaxaca, consulté le septembre 14, 2025, https://www.statsmodels.org/stable/_modules/statsmodels/stats/oaxaca.html
